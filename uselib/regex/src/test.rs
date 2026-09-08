


#[cfg(test)]
mod tests {
    use crate::Regex;

    #[test]
    fn test_regex() {
        let inline_var = Regex::new(r"abc").unwrap(); // 一度だけコンパイルして使い回してOK

        let value = "abc abc";
        let asm = inline_var.replace_all(value, "aaa");

        assert_eq!(asm, "aaa aaa");
    }

    #[test]
    fn test_2_regex() {
        let inline_var = Regex::new(r"\$\{aa\}").unwrap(); // 一度だけコンパイルして使い回してOK

        let value = "${aa} is ${aa}";

        let asm = inline_var.replace_all(value, "tomato");
        assert_eq!(asm, "tomato is tomato");
    }

    #[test]
    fn test_cond_regex() {
        let inline_var = Regex::new(r"\$\{([^}]+)\}").unwrap(); // 一度だけコンパイルして使い回してOK

        let value = "${aa} is ${aa}";

        let asm = inline_var.replace_all(value, "tomato");
        assert_eq!(asm, "tomato is tomato");
    }
}