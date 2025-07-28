use std::marker::PhantomData;
use std::str::FromStr;
use tree_sitter::Node;

pub struct NodeKinded<'a, K> {
    children: Vec<Node<'a>>,
    index: usize,
    phantom: PhantomData<K>,
}

impl<'a, K> NodeKinded<'a, K>
where
    K: FromStr,
{
    pub fn new(parent: Node<'a>) -> Self {
        let children: Vec<_> = parent.children(&mut parent.walk()).collect();
        Self {
            children,
            index: 0,
            phantom: PhantomData,
        }
    }
}

impl<'a, K> Iterator for NodeKinded<'a, K>
where
    K: FromStr,
{
    type Item = (K, Node<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        while self.index < self.children.len() {
            let node = self.children[self.index];
            self.index += 1;

            if let Ok(kind) = node.kind().parse::<K>() {
                return Some((kind, node));
            }
        }
        None
    }
}

pub trait NodeKindedExt<'a> {
    fn kinds<K>(&'a self) -> NodeKinded<'a, K>
    where
        K: FromStr;
}

impl<'a> NodeKindedExt<'a> for Node<'a> {
    fn kinds<K>(&'a self) -> NodeKinded<'a, K>
    where
        K: FromStr,
    {
        NodeKinded::new(*self)
    }
}
