use std::env;

fn greet(name: &str) -> String {
    format!("hello from __NAME__, {name}")
}

fn main() {
    let name = env::args().nth(1).unwrap_or_else(|| "world".to_string());
    println!("{}", greet(&name));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greets() {
        assert_eq!(greet("world"), "hello from __NAME__, world");
    }
}
