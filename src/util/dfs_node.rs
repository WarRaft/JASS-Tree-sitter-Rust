use tree_sitter::Node;

pub struct Dfs<'tree> {
    stack: Vec<(Node<'tree>, usize)>,
}

impl<'tree> Dfs<'tree> {
    pub(crate) fn new(root: Node<'tree>) -> Self {
        Dfs {
            stack: vec![(root, 0)],
        }
    }
}

impl<'tree> Iterator for Dfs<'tree> {
    type Item = Node<'tree>;

    fn next(&mut self) -> Option<Self::Item> {
        while let Some((node, index)) = self.stack.pop() {
            if index == 0 {
                let child_count = node.child_count();
                if child_count > 0 {
                    self.stack.push((node, 1));
                    for i in 0..child_count {
                        if let Some(child) = node.child(i) {
                            self.stack.push((child, 0));
                            break;
                        }
                    }
                }
                return Some(node);
            } else {
                let total = node.child_count();
                let mut i = index;
                while i < total {
                    if let Some(child) = node.child(i) {
                        self.stack.push((node, i + 1));
                        self.stack.push((child, 0));
                        break;
                    }
                    i += 1;
                }
            }
        }
        None
    }
}
