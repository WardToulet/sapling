use super::cursor_path::CursorPath;
use super::{Direction, EditableTree};
use crate::arena::Arena;
use crate::ast::Ast;

/// An [`EditableTree`] that stores the history as a DAG (Directed Acyclic Graph) of **immutable**
/// nodes.
///
/// This means that every node that has ever been created exists somewhere in the DAG, and when
/// changes are made, every ancestor of that node is cloned until the root is reached and that
/// root becomes the new 'current' root.  This is very similar to the way Git stores the commits,
/// and every edit is analogous to a Git rebase.
///
/// Therefore, moving back through the history is as simple as reading a different root node from
/// the `roots` vector, and following its descendants through the DAG of nodes.
pub struct DAG<'arena, Node: Ast<'arena>> {
    /// The arena in which all the [`Node`]s will be stored
    arena: &'arena Arena<Node>,
    /// A [`Vec`] containing a reference to the root node at every edit in the undo history.  This
    /// is required to always have length at least one.
    root_history: Vec<(&'arena Node, CursorPath)>,
    /// An index into [`root_history`](DAG::root_history) of the current edit.  This is required to
    /// be in `0..root_history.len()`.
    history_index: usize,
    current_cursor_path: CursorPath,
}

impl<'arena, Node: Ast<'arena>> DAG<'arena, Node> {
    /// Returns the cursor node and its direct parent (if such a parent exists)
    fn cursor_and_parent(&self) -> (&'arena Node, Option<&'arena Node>) {
        self.current_cursor_path.cursor_and_parent(self.root())
    }
}

impl<'arena, Node: Ast<'arena>> EditableTree<'arena, Node> for DAG<'arena, Node> {
    fn new(arena: &'arena Arena<Node>, root: &'arena Node) -> Self {
        DAG {
            arena,
            root_history: vec![(root, CursorPath::root())],
            history_index: 0,
            current_cursor_path: CursorPath::root(),
        }
    }

    /* HISTORY METHODS */

    fn undo(&mut self) -> bool {
        if self.history_index > 0 {
            self.history_index -= 1;
            // Follow the behaviour of other text editors and update the location of the cursor
            // with its location in the snapshot we are going back to
            self.current_cursor_path
                .clone_from(&self.root_history[self.history_index].1);
            true
        } else {
            false
        }
    }

    fn redo(&mut self) -> bool {
        if self.history_index < self.root_history.len() - 1 {
            self.history_index += 1;
            // Follow the behaviour of other text editors and update the location of the cursor
            // with its location in the snapshot we are going back to
            self.current_cursor_path
                .clone_from(&self.root_history[self.history_index].1);
            true
        } else {
            false
        }
    }

    /* NAVIGATION METHODS */

    fn root(&self) -> &'arena Node {
        // This indexing shouldn't panic because we require that `self.history_index` is a valid index
        // into `self.root_history`, and `self.root_history` has at least one element
        self.root_history[self.history_index].0
    }

    fn cursor(&self) -> &'arena Node {
        self.current_cursor_path.cursor(self.root())
    }

    fn move_cursor(&mut self, direction: Direction) -> Option<String> {
        let (current_cursor, cursor_parent) = self.cursor_and_parent();
        match direction {
            Direction::Down => {
                if current_cursor.children().is_empty() {
                    Some("Cannot move down the tree if the cursor has no children.".to_string())
                } else {
                    self.current_cursor_path.push(0);
                    None
                }
            }
            Direction::Up => {
                if self.current_cursor_path.is_root() {
                    return Some("Cannot move to the parent of the root.".to_string());
                }
                self.current_cursor_path.pop();
                None
            }
            Direction::Prev => {
                if let Some(index) = self.current_cursor_path.last_mut() {
                    if *index == 0 {
                        Some("Cannot move before the first child of a node.".to_string())
                    } else {
                        *index -= 1;
                        None
                    }
                } else {
                    Some("Cannot move to a sibling of the root.".to_string())
                }
            }
            Direction::Next => {
                if let Some(last_index) = self.current_cursor_path.last_mut() {
                    // We can unwrap here, because the only way for a node to not have a parent is
                    // if it's the root.  And if the cursor is at the root, then the `if let` would
                    // have failed and this code would not be run.
                    if *last_index + 1 < cursor_parent.unwrap().children().len() {
                        *last_index += 1;
                        None
                    } else {
                        Some("Cannot move past the last sibling of a node.".to_string())
                    }
                } else {
                    Some("Cannot move to a sibling of the root.".to_string())
                }
            }
        }
    }

    fn replace_cursor(&mut self, new_node: Node) {
        // Remove future trees from the history vector so that the currently 'checked-out' tree is
        // the most recent tree in the history.
        while self.history_index < self.root_history.len() - 1 {
            // TODO: Deallocate the tree so that we don't get a 'memory leak'
            self.root_history.pop();
        }
        // Generate a vec of pointers to the nodes that we will have to clone.  We have to store
        // this as a vec because the iterator that produces them (cursor_path::NodeIter) can only
        // yield values from the root downwards, whereas we need the nodes in the opposite order.
        let mut nodes_to_clone: Vec<_> = self.current_cursor_path.node_iter(self.root()).collect();
        // The last value of nodes_to_clone is the node under the cursor, which we do not need to
        // clone, so we pop that reference.
        assert!(nodes_to_clone.pop().is_some());
        /* Because AST nodes are immutable, we make changes to nodes by entirely cloning the path
         * down to the node under the cursor.  We do this starting at the node under the cursor and
         * work our way up parent by parent until we reach the root of the tree.  At that point,
         * this node becomes the root of the new tree.
         */
        let mut node = self.arena.alloc(new_node);
        // Iterate backwards over the child indices and the nodes, whilst cloning the tree and
        // replacing the correct child reference to point to the newly created node.
        for (n, child_index) in nodes_to_clone
            .iter()
            .rev()
            .zip(self.current_cursor_path.iter().rev())
        {
            let mut cloned_node = (*n).clone();
            cloned_node.children_mut()[*child_index] = node;
            node = self.arena.alloc(cloned_node);
        }
        // At this point, `node` contains a reference to the root of the new tree, so we just add
        // this to the history, along with the cursor path.
        self.root_history
            .push((node, self.current_cursor_path.clone()));
        // Move the history index on by one so that we are pointing at the latest change
        self.history_index = self.root_history.len() - 1;
    }

    fn insert_child(&mut self, _new_node: Node) {
        unimplemented!();
    }

    fn write_text(&self, string: &mut String, format: &Node::FormatStyle) {
        self.root().write_text(string, format);
    }
}
