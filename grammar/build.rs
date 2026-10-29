// Generates the parser from atls.lalrpop; LALRPOP fails, and with it the build, on any conflict.
fn main() {
    let out = std::env::var("OUT_DIR").expect("OUT_DIR");
    lalrpop::Configuration::new()
        .set_in_dir(".")
        .set_out_dir(out)
        .process_file("atls.lalrpop")
        .expect("the grammar has no conflict");
    println!("cargo:rerun-if-changed=atls.lalrpop");
}
