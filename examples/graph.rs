#![feature(allocator_api)]

pub mod graph{
    use std::{rc::Rc};
    use bump_recycle::ReBump;

    pub struct Node{
        out: Vec<usize/* node index */, Rc<ReBump>>
    }
    impl Node{
        pub fn out(&self) -> &[usize]{
            &self.out
        }
        pub fn push_out(&mut self, idx: usize){
            self.out.push(idx);
        }
    }

    pub struct Graph{
        nodes: Vec<Node, Rc<ReBump>>
    }

    // Rc prevents us from being Send safe.
    // But, Rc<ReBump> is inaccessible from outside, and ReBump is Send safe.
    // It is impossible to change Rc counters, or access it's containment
    // from different threads.
    unsafe impl Send for Graph{}

    impl Graph{
        pub fn new() -> Graph{
            Graph{
                nodes: Vec::new_in(Default::default()),
            }
        }

        pub fn nodes(&self) -> &[Node] {
            &self.nodes
        }

        pub fn nodes_mut(&mut self) -> &mut [Node] {
            &mut self.nodes
        }

        pub fn push_node(&mut self) -> usize {
            let allocator = self.nodes.allocator().clone();
            let idx = self.nodes.len();
            self.nodes.push(Node{
                out: Vec::new_in(allocator),
            });
            idx
        }
    }
}

fn main(){
    use graph::*;
    let mut graph = Graph::new();
    let n0_idx = graph.push_node();
    let n1_idx = graph.push_node();
    graph.nodes_mut()[n0_idx].push_out(n1_idx);
}