// move_semantics2.rs
// Make me compile without changing line 13!
// Execute `rustlings hint move_semantics2` for hints :)

fn main() {
    let vec0 = Vec::new();

    let mut vec1 = fill_vec(vec0);

    // enclosing in this block makes vec0 out of scope
    // when we push to vec1 so borrow should be returned by then
    {
        let vec0 = &vec1;
        // Do not change the following line!
        println!("{} has length {} content `{:?}`", "vec0", vec0.len(), vec0);
    }
    

    vec1.push(88);

    println!("{} has length {} content `{:?}`", "vec1", vec1.len(), vec1);
}

fn fill_vec(vec: Vec<i32>) -> Vec<i32> {
    let mut vec = vec;

    vec.push(22);
    vec.push(44);
    vec.push(66);

    vec
}
