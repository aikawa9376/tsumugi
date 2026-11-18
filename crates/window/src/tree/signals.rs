//! The rows' shared tree facts and the selection facts of one row.

use dioxus::prelude::*;
use tsumugi_core::tree::{TreeView, nav::Item};
use tsumugi_model::EnvironmentId;

pub(crate) fn use_structure(tree: Signal<TreeView>) -> Memo<TreeView> {
    use_memo(move || {
        let mut structure = tree.read().clone();
        // Each row follows its own selection below; including it here would
        // redraw every row for each move through the sidebar.
        structure.selected = None;
        structure
    })
}

#[derive(Clone, PartialEq)]
pub(crate) struct RowFocus {
    pub selected: bool,
    pub renames_here: bool,
    pub environment: Option<EnvironmentId>,
}

pub(crate) fn use_row_focus(tree: Signal<TreeView>, item: Item) -> Memo<RowFocus> {
    use_memo(move || RowFocus::of(&tree.read(), &item))
}

impl RowFocus {
    fn of(tree: &TreeView, item: &Item) -> Self {
        let selected = tree.selected.as_ref() == Some(item);
        Self {
            selected,
            renames_here: selected
                || (!matches!(item, Item::Gathered(_))
                    && tree.selected.as_ref().map(Item::own) != Some(item.own())),
            environment: tree.selected.as_ref().map(|s| s.environment().clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    #[derive(Clone)]
    struct Observed {
        tree: Rc<Cell<Option<Signal<TreeView>>>>,
        draws: Rc<RefCell<Vec<usize>>>,
    }

    fn item(index: usize) -> Item {
        Item::Session(EnvironmentId::unbound(), index.to_string())
    }

    #[component]
    fn Row(index: usize, tree: Signal<TreeView>, structure: Memo<TreeView>) -> Element {
        let observed = use_context::<Observed>();
        let focus = use_row_focus(tree, item(index));
        let _ = (structure.read(), focus.read());
        observed.draws.borrow_mut()[index] += 1;
        rsx! { div { "{focus.read().selected}" } }
    }

    #[component]
    fn Frame() -> Element {
        let observed = use_context::<Observed>();
        let tree = use_signal(|| {
            let mut tree = TreeView::default();
            tree.selected = Some(item(0));
            tree
        });
        observed.tree.set(Some(tree));
        let structure = use_structure(tree);
        rsx! {
            for index in 0..40 { Row { key: "{index}", index, tree, structure } }
        }
    }

    #[tokio::test]
    async fn moving_the_selection_redraws_only_the_rows_it_leaves_and_reaches() {
        let observed = Observed {
            tree: Rc::new(Cell::new(None)),
            draws: Rc::new(RefCell::new(vec![0; 40])),
        };
        let mut dom = VirtualDom::new(Frame).with_root_context(observed.clone());
        dom.rebuild_in_place();
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        let before = observed.draws.borrow().clone();
        dom.in_runtime(|| {
            observed.tree.get().unwrap().write().selected = Some(item(1));
        });
        dom.wait_for_work().await;
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        {
            let after = observed.draws.borrow();
            let changed: Vec<_> = (0..40).filter(|&i| after[i] > before[i]).collect();
            assert_eq!(changed, [0, 1]);
        }

        // A fold or listing change still reaches every row that reads the
        // shared tree facts.
        let before = observed.draws.borrow().clone();
        dom.in_runtime(|| {
            observed.tree.get().unwrap().write().folds.set(
                &Item::Workspace(EnvironmentId::unbound(), "/project".into()),
                false,
            );
        });
        dom.wait_for_work().await;
        dom.render_immediate(&mut dioxus::core::NoOpMutations);
        assert!((0..40).all(|i| observed.draws.borrow()[i] > before[i]));
    }

    #[test]
    fn duplicate_rows_keep_selection_and_renaming_on_the_right_occurrence() {
        let own = item(0);
        let gathered = Item::Gathered(Box::new(own.clone()));
        let mut tree = TreeView::default();
        for (selected, expected) in [
            (None, [(false, true), (false, false)]),
            (Some(own.clone()), [(true, true), (false, false)]),
            (Some(gathered.clone()), [(false, false), (true, true)]),
            (Some(item(1)), [(false, true), (false, false)]),
        ] {
            tree.selected = selected;
            for (row, (selected, renames_here)) in [&own, &gathered].into_iter().zip(expected) {
                let focus = RowFocus::of(&tree, row);
                assert_eq!(
                    (focus.selected, focus.renames_here),
                    (selected, renames_here)
                );
                assert_eq!(
                    focus.environment,
                    tree.selected.as_ref().map(|s| s.environment().clone())
                );
            }
        }
    }
}
