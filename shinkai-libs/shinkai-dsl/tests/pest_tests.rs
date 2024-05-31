#[cfg(test)]
mod tests {
    use pest::Parser;
    use identifier_parser::Rule;

    mod identifier_parser {
        use pest::Parser;
        use pest_derive::Parser;

        #[derive(Parser)]
        #[grammar_inline = r#"
        identifier = _{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
        "#]
        pub struct IdentifierParser;
    }

    mod input_parser {
        use super::identifier_parser::IdentifierParser;
        use pest::Parser;
        use pest_derive::Parser;

        #[derive(Parser)]
        #[grammar_inline = r#"
        input = { "input" ~ identifier ~ ":" ~ identifier }
        identifier = _{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
        "#]
        pub struct InputParser;
    }

    mod output_parser {
        use super::identifier_parser::IdentifierParser;
        use pest::Parser;
        use pest_derive::Parser;

        #[derive(Parser)]
        #[grammar_inline = r#"
        output = { "output" ~ identifier ~ ":" ~ identifier }
        identifier = _{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
        "#]
        pub struct OutputParser;
    }

    mod condition_parser {
        use super::identifier_parser::IdentifierParser;
        use pest::Parser;
        use pest_derive::Parser;

        #[derive(Parser)]
        #[grammar_inline = r#"
        condition = { "if" ~ expression }
        expression = { identifier ~ comparison_operator ~ value }
        comparison_operator = { "==" | "!=" | ">" | "<" | ">=" | "<=" }
        value = { string | number | boolean | identifier }
        identifier = _{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
        string = _{ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
        number = _{ ASCII_DIGIT+ }
        boolean = { "true" | "false" }
        "#]
        pub struct ConditionParser;
    }

    mod action_parser {
        use super::identifier_parser::IdentifierParser;
        use pest::Parser;
        use pest_derive::Parser;

        #[derive(Parser)]
        #[grammar_inline = r#"
        action = { command ~ "(" ~ (param ~ ("," ~ param)*)? ~ ")" | external_fn_call }
        command = { identifier }
        param = { string | number | boolean | identifier }
        external_fn_call = { "call" ~ identifier ~ "(" ~ (param ~ ("," ~ param)*)? ~ ")" }
        identifier = _{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
        string = _{ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
        number = _{ ASCII_DIGIT+ }
        boolean = { "true" | "false" }
        "#]
        pub struct ActionParser;
    }

    mod step_parser {
        use super::action_parser::ActionParser;
        use super::condition_parser::ConditionParser;
        use pest::Parser;
        use pest_derive::Parser;

        #[derive(Parser)]
        #[grammar_inline = r#"
        step = { "step" ~ identifier ~ "{" ~ step_body ~ "}" }
        step_body = { condition? ~ action }
        condition = { "if" ~ expression }
        action = { command ~ "(" ~ (param ~ ("," ~ param)*)? ~ ")" | external_fn_call }
        command = { identifier }
        param = { string | number | boolean | identifier }
        external_fn_call = { "call" ~ identifier ~ "(" ~ (param ~ ("," ~ param)*)? ~ ")" }
        expression = { identifier ~ comparison_operator ~ value }
        comparison_operator = { "==" | "!=" | ">" | "<" | ">=" | "<=" }
        value = { string | number | boolean | identifier }
        identifier = _{ (ASCII_ALPHA | "_") ~ (ASCII_ALPHANUMERIC | "_")* }
        string = _{ "\"" ~ (!"\"" ~ ANY)* ~ "\"" }
        number = _{ ASCII_DIGIT+ }
        boolean = { "true" | "false" }
        "#]
        pub struct StepParser;
    }

    #[test]
    fn test_identifier() {
        let valid_identifiers = vec!["agent1", "my_agent", "_agent"];
        for id in valid_identifiers {
            assert!(identifier_parser::IdentifierParser::parse(Rule::identifier, id).is_ok());
        }
    }

    #[test]
    fn test_input() {
        use input_parser::InputParser;
        use input_parser::Rule;

        let input_str = "input topic: identifier";
        let parse_result = InputParser::parse(Rule::input, input_str);
        assert!(parse_result.is_ok());
    }

    #[test]
    fn test_output() {
        use output_parser::OutputParser;
        use output_parser::Rule;
    
        let output_str = "output perspectives: List<String>";
        let parse_result = OutputParser::parse(Rule::output, output_str);
        assert!(parse_result.is_ok());
    }

    #[test]
    fn test_condition() {
        use condition_parser::ConditionParser;
        use condition_parser::Rule;

        let condition_str = "if perspectives != \"\"";
        let parse_result = ConditionParser::parse(Rule::condition, condition_str);
        assert!(parse_result.is_ok());
    }

    #[test]
    fn test_action() {
        use action_parser::ActionParser;
        use action_parser::Rule;

        let action_str = "generate_questions(perspectives)";
        let parse_result = ActionParser::parse(Rule::action, action_str);
        assert!(parse_result.is_ok());
    }

    #[test]
    fn test_step() {
        use step_parser::StepParser;
        use step_parser::Rule;

        let step_str = r#"
        step GenerateQuestions {
            if perspectives != "" {
                generate_questions(perspectives)
            }
        }
        "#;
        let parse_result = StepParser::parse(Rule::step, step_str);
        assert!(parse_result.is_ok());
    }
}