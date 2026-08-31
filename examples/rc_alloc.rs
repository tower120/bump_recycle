#![feature(allocator_api)]

use std::rc::Rc;
use recycled_bump::ReBump;

// There is no benefits of using Arc here - since Arc would be Send-able,
// only if ReBump would be Sync-able.
struct S{
    vec1: Vec<u32, Rc<ReBump>>,
    vec2: Vec<f32, Rc<ReBump>>
}

impl S{
    pub fn new() -> Self{
        let alloc = Rc::new(ReBump::new());
        Self{
            vec1: Vec::new_in(alloc.clone()),
            vec2: Vec::new_in(alloc.clone()),
        }
    }
}

fn main(){
    let mut s = S::new();
    s.vec1.extend([1,2,3]);
    s.vec1.clear();
    s.vec1.shrink_to_fit();

    // vec2 will now internally reuse memory chunk, previously allocated by vec1
    s.vec2.extend([1.0,2.0,3.0]);
}