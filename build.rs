use parol::build::Builder;

fn main() {
    // Generate parser and user trait code using parol
    Builder::with_cargo_script_output().grammar_file("src/query/logql.par")
        .generate_parser()
        .unwrap(); // Handle error
}
