---
title: "Children"
---

## General usage

_Most of the time,_ when allowing a component to have children, you don't care 
what type of children the component has. In such cases, the below example will
suffice.

```rust
use yew::prelude::*;

#[derive(Properties, PartialEq)]
pub struct ListProps {
    #[prop_or_default]
    pub children: Children,
}

pub struct List;

impl Component for List {
    type Properties = ListProps;
    // ...

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="list">
                { for ctx.props().children.iter() }
            </div>
        }
    }
}
```

## Advanced usage

### Typed children
In cases where you want one type of component to be passed as children to your component,
you can use `yew::html::ChildrenWithProps<T>`.

```rust
use yew::html::ChildrenWithProps;
use yew::prelude::*;

// ...

#[derive(Properties, PartialEq)]
pub struct ListProps {
    #[prop_or_default]
    pub children: ChildrenWithProps<Item>,
}

pub struct List;

impl Component for ListProps {
    type Properties = ListProps;
    // ...

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="list">
                { for ctx.props().children.iter() }
            </div>
        }
    }
}
```

### Enum typed children
Of course, sometimes you might need to restrict the children to a few different
components. In these cases, you have to get a little more hands-on with Yew.

The [`derive_more`](https://github.com/JelteF/derive_more) crate is used here
for better ergonomics. If you don't want to use it, you can manually implement
`From` for each variant.

```rust
use yew::prelude::*;
use yew::html::ChildrenRenderer;
use yew::virtual_dom::{ VChild, VComp };

// `derive_more::From` implements `From<VChild<Primary>>` and
// `From<VChild<Secondary>>` for `Item` automatically!
#[derive(Clone, derive_more::From)]
pub enum Item {
    Primary(VChild<Primary>),
    Secondary(VChild<Secondary>),
}

// Now, we implement `Into<Html>` so that yew knows how to render `Item`.
impl Into<Html> for Item {
    fn into(self) -> Html {
        match self {
            Self::Primary(child) => child.into(),
            Self::Secondary(child) => child.into(),
        }
    }
}

#[derive(Properties, PartialEq)]
pub struct ListProps {
    #[prop_or_default]
    pub children: ChildrenRenderer<Item>,
}

pub struct List;

impl Component for List {
    type Properties = ListProps;
    // ...

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="list">
                { for ctx.props().children.iter() }
            </div>
        }
    }
}
```

### Optional typed child
You can also have a single optional child component of a specific type too: 

```rust
use yew::prelude::*;
use yew::virtual_dom::VChild;


#[derive(Properties, PartialEq)]
pub struct PageProps {
    #[prop_or_default]
    pub sidebar: Option<VChild<PageSideBar>>,
}

struct Page;

impl Component for Page {
    type Properties = PageProps;
    // ...

    fn view(&self, ctx: &Context<Self>) -> Html {
        html! {
            <div class="page">
                { ctx.props().sidebar.clone().map(Html::from).unwrap_or_default() }
                // ... page content
            </div>
        }
    }
}
```

The page component can be called either with the sidebar or without: 

```rust
    // Page without sidebar
    html! {
        <Page />
    }

    // Page with sidebar
    html! {
        <Page sidebar={html_nested! {
            <PageSideBar />
        }} />
    }
```
