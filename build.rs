use parol::build::Builder;

fn main() {
    // Generate parser and user trait code using parol
    Builder::with_cargo_script_output().with_grammar_file("src/query/logql.par")
        .unwrap() // Handle error appropriately in real code
        .generate_parser_source()
        .unwrap(); // Handle error
}
