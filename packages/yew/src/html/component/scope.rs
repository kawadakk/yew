//! Component scope module

use super::{
    lifecycle::{
        ComponentLifecycleEvent, ComponentRunnable, ComponentState, CreateEvent, UpdateEvent,
    },
    Component,
};
use crate::callback::Callback;
use crate::context::{ContextHandle, ContextProvider};
use crate::html::NodeRef;
use crate::scheduler::{self, Shared};
use crate::utils::document;
use crate::virtual_dom::{insert_node, VNode};
use std::any::{Any, TypeId};
use std::cell::{Ref, RefCell};
use std::future::Future;
use std::ops::Deref;
use std::rc::Rc;
use std::{fmt, iter};
use wasm_bindgen_futures::spawn_local;
use web_sys::{Element, Node};

/// Untyped scope used for accessing parent scope
#[derive(Debug, Clone)]
pub struct AnyScope {
    type_id: TypeId,
    parent: Option<Rc<AnyScope>>,
    state: Rc<dyn Any>,
}

impl<COMP: Component> From<Scope<COMP>> for AnyScope {
    fn from(scope: Scope<COMP>) -> Self {
        AnyScope {
            type_id: TypeId::of::<COMP>(),
            parent: scope.parent,
            state: scope.state,
        }
    }
}

impl AnyScope {
    #[cfg(test)]
    pub(crate) fn test() -> Self {
        Self {
            type_id: TypeId::of::<()>(),
            parent: None,
            state: Rc::new(()),
        }
    }

    /// Returns the parent scope
    pub fn get_parent(&self) -> Option<&AnyScope> {
        self.parent.as_deref()
    }

    /// Returns the type of the linked component
    pub fn get_type_id(&self) -> &TypeId {
        &self.type_id
    }

    /// Attempts to downcast into a typed scope
    pub fn downcast<COMP: Component>(self) -> Scope<COMP> {
        Scope {
            parent: self.parent,
            state: self
                .state
                .downcast::<RefCell<Option<ComponentState<COMP>>>>()
                .expect("unexpected component type"),
        }
    }

    fn find_parent_scope<C: Component>(&self) -> Option<Scope<C>> {
        let expected_type_id = TypeId::of::<C>();
        iter::successors(Some(self), |scope| scope.get_parent())
            .filter(|scope| scope.get_type_id() == &expected_type_id)
            .cloned()
            .map(AnyScope::downcast::<C>)
            .next()
    }

    /// Accesses a value provided by a parent `ContextProvider` component of the
    /// same type.
    pub fn context<T: Clone + PartialEq + 'static>(
        &self,
        callback: Callback<T>,
    ) -> Option<(T, ContextHandle<T>)> {
        let scope = self.find_parent_scope::<ContextProvider<T>>()?;
        let scope_clone = scope.clone();
        let component = scope.get_component()?;
        Some(component.subscribe_consumer(callback, scope_clone))
    }
}

pub(crate) trait Scoped {
    fn to_any(&self) -> AnyScope;
    fn root_vnode(&self) -> Option<Ref<'_, VNode>>;
    fn destroy(&mut self);
}

impl<COMP: Component> Scoped for Scope<COMP> {
    fn to_any(&self) -> AnyScope {
        self.clone().into()
    }

    fn root_vnode(&self) -> Option<Ref<'_, VNode>> {
        let state_ref = self.state.borrow();

        // check that component hasn't been destroyed
        state_ref.as_ref()?;

        Some(Ref::map(state_ref, |state_ref| {
            &state_ref.as_ref().unwrap().root_node
        }))
    }

    /// Process an event to destroy a component
    fn destroy(&mut self) {
        self.process(ComponentLifecycleEvent::Destroy);
    }
}

/// A context which allows sending messages to a component.
pub struct Scope<COMP: Component> {
    parent: Option<Rc<AnyScope>>,
    state: Shared<Option<ComponentState<COMP>>>,
}

impl<COMP: Component> fmt::Debug for Scope<COMP> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Scope<_>")
    }
}

impl<COMP: Component> Clone for Scope<COMP> {
    fn clone(&self) -> Self {
        Scope {
            parent: self.parent.clone(),
            state: self.state.clone(),
        }
    }
}

impl<COMP: Component> Scope<COMP> {
    /// Returns the parent scope
    pub fn get_parent(&self) -> Option<&AnyScope> {
        self.parent.as_deref()
    }

    /// Returns the linked component if available
    pub fn get_component(&self) -> Option<impl Deref<Target = COMP> + '_> {
        self.state.try_borrow().ok().and_then(|state_ref| {
            state_ref.as_ref()?;
            Some(Ref::map(state_ref, |state| {
                state.as_ref().unwrap().component.as_ref()
            }))
        })
    }

    pub(crate) fn new(parent: Option<AnyScope>) -> Self {
        let parent = parent.map(Rc::new);
        let state = Rc::new(RefCell::new(None));
        Scope { parent, state }
    }

    /// Mounts a component with `props` to the specified `element` in the DOM.
    pub(crate) fn mount_in_place(
        &self,
        parent: Element,
        next_sibling: NodeRef,
        node_ref: NodeRef,
        props: Rc<COMP::Properties>,
    ) {
        let placeholder = {
            let placeholder: Node = document().create_text_node("").into();
            insert_node(&placeholder, &parent, next_sibling.get().as_ref());
            node_ref.set(Some(placeholder.clone()));
            VNode::VRef(placeholder)
        };

        self.schedule(UpdateEvent::First.into());
        self.process(ComponentLifecycleEvent::Create(CreateEvent {
            parent,
            next_sibling,
            placeholder,
            node_ref,
            props,
            scope: self.clone(),
        }));
    }

    pub(crate) fn reuse(
        &self,
        props: Rc<COMP::Properties>,
        node_ref: NodeRef,
        next_sibling: NodeRef,
    ) {
        self.process(UpdateEvent::Properties(props, node_ref, next_sibling).into());
    }

    pub(crate) fn process(&self, event: ComponentLifecycleEvent<COMP>) {
        self.schedule(event);
        scheduler::start();
    }

    fn schedule(&self, event: ComponentLifecycleEvent<COMP>) {
        use ComponentLifecycleEvent::*;

        let push = match &event {
            Create(_) => scheduler::push_component_create,
            Update(_) => scheduler::push_component_update,
            Render => scheduler::push_component_render,
            Rendered => scheduler::push_component_rendered,
            Destroy => scheduler::push_component_destroy,
        };
        push(Box::new(ComponentRunnable {
            state: self.state.clone(),
            event,
        }));
    }

    /// Send a message to the component.
    ///
    /// Please be aware that currently this method synchronously
    /// schedules a call to the [Component](Component) interface.
    pub fn send_message<T>(&self, msg: T)
    where
        T: Into<COMP::Message>,
    {
        self.process(UpdateEvent::Message(msg.into()).into());
    }

    /// Send a batch of messages to the component.
    ///
    /// This is useful for reducing re-renders of the components
    /// because the messages are handled together and the view
    /// function is called only once if needed.
    ///
    /// Please be aware that currently this method synchronously
    /// schedules calls to the [Component](Component) interface.
    pub fn send_message_batch(&self, messages: Vec<COMP::Message>) {
        // There is no reason to schedule empty batches.
        // This check is especially handy for the batch_callback method.
        if messages.is_empty() {
            return;
        }

        self.process(UpdateEvent::MessageBatch(messages).into());
    }

    /// Creates a `Callback` which will send a message to the linked
    /// component's update method when invoked.
    ///
    /// Please be aware that currently the result of this callback
    /// synchronously schedules a call to the [Component](Component)
    /// interface.
    pub fn callback<F, IN, M>(&self, function: F) -> Callback<IN>
    where
        M: Into<COMP::Message>,
        F: Fn(IN) -> M + 'static,
    {
        let scope = self.clone();
        let closure = move |input| {
            let output = function(input);
            scope.send_message(output);
        };
        closure.into()
    }

    /// Creates a `Callback` from an `FnOnce` which will send a message
    /// to the linked component's update method when invoked.
    ///
    /// Please be aware that currently the result of this callback
    /// will synchronously schedule calls to the
    /// [Component](Component) interface.
    pub fn callback_once<F, IN, M>(&self, function: F) -> Callback<IN>
    where
        M: Into<COMP::Message>,
        F: FnOnce(IN) -> M + 'static,
    {
        let scope = self.clone();
        let closure = move |input| {
            let output = function(input);
            scope.send_message(output);
        };
        Callback::once(closure)
    }

    /// Creates a `Callback` which will send a batch of messages back
    /// to the linked component's update method when invoked.
    ///
    /// The callback function's return type is generic to allow for dealing with both
    /// `Option` and `Vec` nicely. `Option` can be used when dealing with a callback that
    /// might not need to send an update.
    ///
    /// ```ignore
    /// link.batch_callback(|_| vec![Msg::A, Msg::B]);
    /// link.batch_callback(|_| Some(Msg::A));
    /// ```
    ///
    /// Please be aware that currently the results of these callbacks
    /// will synchronously schedule calls to the
    /// [Component](Component) interface.
    pub fn batch_callback<F, IN, OUT>(&self, function: F) -> Callback<IN>
    where
        F: Fn(IN) -> OUT + 'static,
        OUT: SendAsMessage<COMP>,
    {
        let scope = self.clone();
        let closure = move |input| {
            let messages = function(input);
            messages.send(&scope);
        };
        closure.into()
    }

    /// Creates a `Callback` from an `FnOnce` which will send a batch of messages back
    /// to the linked component's update method when invoked.
    ///
    /// The callback function's return type is generic to allow for dealing with both
    /// `Option` and `Vec` nicely. `Option` can be used when dealing with a callback that
    /// might not need to send an update.
    ///
    /// ```ignore
    /// link.batch_callback_once(|_| vec![Msg::A, Msg::B]);
    /// link.batch_callback_once(|_| Some(Msg::A));
    /// ```
    ///
    /// Please be aware that currently the results of these callbacks
    /// will synchronously schedule calls to the
    /// [Component](Component) interface.
    pub fn batch_callback_once<F, IN, OUT>(&self, function: F) -> Callback<IN>
    where
        F: FnOnce(IN) -> OUT + 'static,
        OUT: SendAsMessage<COMP>,
    {
        let scope = self.clone();
        let closure = move |input| {
            let messages = function(input);
            messages.send(&scope);
        };
        Callback::once(closure)
    }

    /// This method creates a [`Callback`] which returns a Future which
    /// returns a message to be sent back to the component's event
    /// loop.
    ///
    /// # Panics
    /// If the future panics, then the promise will not resolve, and
    /// will leak.
    pub fn callback_future<FN, FU, IN, M>(&self, function: FN) -> Callback<IN>
    where
        M: Into<COMP::Message>,
        FU: Future<Output = M> + 'static,
        FN: Fn(IN) -> FU + 'static,
    {
        let link = self.clone();

        let closure = move |input: IN| {
            let future: FU = function(input);
            link.send_future(future);
        };

        closure.into()
    }

    /// This method creates a [`Callback`] from [`FnOnce`] which returns a Future
    /// which returns a message to be sent back to the component's event
    /// loop.
    ///
    /// # Panics
    /// If the future panics, then the promise will not resolve, and
    /// will leak.
    pub fn callback_future_once<FN, FU, IN, M>(&self, function: FN) -> Callback<IN>
    where
        M: Into<COMP::Message>,
        FU: Future<Output = M> + 'static,
        FN: FnOnce(IN) -> FU + 'static,
    {
        let link = self.clone();

        let closure = move |input: IN| {
            let future: FU = function(input);
            link.send_future(future);
        };

        Callback::once(closure)
    }

    /// This method processes a Future that returns a message and sends it back to the component's
    /// loop.
    ///
    /// # Panics
    /// If the future panics, then the promise will not resolve, and will leak.
    pub fn send_future<F, M>(&self, future: F)
    where
        M: Into<COMP::Message>,
        F: Future<Output = M> + 'static,
    {
        let link = self.clone();
        let js_future = async move {
            let message: COMP::Message = future.await.into();
            link.send_message(message);
        };
        spawn_local(js_future);
    }

    /// Registers a Future that resolves to multiple messages.
    /// # Panics
    /// If the future panics, then the promise will not resolve, and will leak.
    pub fn send_future_batch<F>(&self, future: F)
    where
        F: Future<Output = Vec<COMP::Message>> + 'static,
    {
        let link = self.clone();
        let js_future = async move {
            let messages: Vec<COMP::Message> = future.await;
            link.send_message_batch(messages);
        };
        spawn_local(js_future);
    }

    /// Accesses a value provided by a parent `ContextProvider` component of the
    /// same type.
    pub fn context<T: Clone + PartialEq + 'static>(
        &self,
        callback: Callback<T>,
    ) -> Option<(T, ContextHandle<T>)> {
        self.to_any().context(callback)
    }
}

/// Defines a message type that can be sent to a component.
/// Used for the return value of closure given to [Scope::batch_callback](struct.Scope.html#method.batch_callback).
pub trait SendAsMessage<COMP: Component> {
    /// Sends the message to the given component's scope.
    /// See [Scope::batch_callback](struct.Scope.html#method.batch_callback).
    fn send(self, scope: &Scope<COMP>);
}

impl<COMP> SendAsMessage<COMP> for Option<COMP::Message>
where
    COMP: Component,
{
    fn send(self, scope: &Scope<COMP>) {
        if let Some(msg) = self {
            scope.send_message(msg);
        }
    }
}

impl<COMP> SendAsMessage<COMP> for Vec<COMP::Message>
where
    COMP: Component,
{
    fn send(self, scope: &Scope<COMP>) {
        scope.send_message_batch(self);
    }
}
