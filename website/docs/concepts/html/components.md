---
title: "Components"
description: "Create complex layouts with component hierarchies"
---

## Basic

Any type that implements `Component` can be used in the `html!` macro:

```rust
html!{
    <>
        // No properties
        <MyComponent />

        // With Properties
        <MyComponent prop1="lorem" prop2="ipsum" />

        // With the whole set of props provided at once
        <MyComponent with props />
    </>
}
```

## Nested

Components can be passed children if they have a `children` field in their `Properties`.

```rust title="parent.rs"
html! {
    <Container id="container">
        <h4>{ "Hi" }</h4>
        <div>{ "Hello" }</div>
    </Container>
}
```

When using the `with props` syntax, the children passed in the `html!` macro overwrite the ones already present in the props.

```rust
let props = yew::props!(Container::Properties {
    id: "container-2",
    children: Children::default(),
});
html! {
    <Container with props>
        // props.children will be overwritten with this
        <span>{ "I am a child, as you can see" }</span>
    </Container>
}
```

Here's the implementation of `Container`:

```rust
#[derive(Properties, Clone)]
pub struct Props {
    pub id: String,
    pub children: Children,
}

pub struct Container;
impl Component for Container {
    type Properties = Props;

    // ...

    fn view(&self, ctx: &Context<Self>) -> Html {
       html! {
           <div id={ctx.props().id.clone()}>
               { ctx.props().children.clone() }
           </div>
       }
    }
}
```

## Nested Children with Props

Nested component properties can be accessed and mutated if the containing component types its children. In the following example, the `List` component can wrap `ListItem` components. For a real world example of this pattern, check out the `yew-router` source code. For a more advanced example, check out the `nested-list` example in the main yew repository.

```rust
html! {
    <List>
        <ListItem value="a" />
        <ListItem value="b" />
        <ListItem value="c" />
    </List>
}
```

```rust
#[derive(Properties, Clone)]
pub struct Props {
    pub children: ChildrenWithProps<ListItem>,
}

pub struct List;
impl Component for List {
    type Properties = Props;

    // ...

    fn view(&self, ctx: &Context<Self>) -> Html {
        html!{{
            for ctx.props().children.iter().map(|mut item| {
                let mut props = Rc::make_mut(&mut item.props);
                props.value = format!("item-{}", props.value);
                item
            })
        }}
    }
}
```

## Relevant examples
- [Boids](https://github.com/yewstack/yew/tree/master/examples/boids)
- [Router](https://github.com/yewstack/yew/tree/master/examples/router)
- [Store](https://github.com/yewstack/yew/tree/master/examples/store)
