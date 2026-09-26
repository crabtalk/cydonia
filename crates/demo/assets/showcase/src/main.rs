mod notes;

fn main() {
    let commits = [
        "feat(board): tag cards while an agent works them",
        "fix(article): keep the caret across a reload",
        "chore: bump dependencies",
    ];
    print!("{}", notes::draft("0.2.0", &commits));
}
