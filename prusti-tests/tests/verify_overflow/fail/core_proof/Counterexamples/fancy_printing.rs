// compile-flags: -Punsafe_core_proof=true -Pcounterexample=true

use prusti_contracts::*;

//#[print_counterexample("text",)]
#[print_counterexample("text {} text {}, {}", b, b, a)]
struct X<T>{
    a: T, 
    b: i32,
}


#[print_counterexample("text {} bla bla", 0)]
struct Y(i32);


#[print_counterexample]
enum Z{
    #[print_counterexample("text {}, {}, {}", g, h, h)]
    E {
        g: i32,
        h: i32,
        i: i32,
    },
    #[print_counterexample("text {}, {} {}", 1, 0 , 1)]
    F(i32, i32),
    #[print_counterexample("text")]
    Unit,
}

#[ensures(!result)]
fn test_mut(x: X<i32>, a: i32, y: Y, z:Z) -> bool{
    prusti_assume!(a > 0);
    x.a + y.0== a
}

fn main() {}