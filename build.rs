extern crate vergen_git2;
use vergen_git2::{BuildBuilder, CargoBuilder, Emitter, Git2Builder};

fn main() {
    let build = BuildBuilder::all_build().unwrap();
    let cargo = CargoBuilder::all_cargo().unwrap();
    let git2 = Git2Builder::all_git().unwrap();
    Emitter::default()
        .add_instructions(&build)
        .unwrap()
        .add_instructions(&cargo)
        .unwrap()
        .add_instructions(&git2)
        .unwrap()
        .emit()
        .unwrap()
}
