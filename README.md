# Concurrent Website Checker

A Rust command-line utility that checks website availability in parallel, measures response times, and writes a JSON summary.

---

## Build Instruction

- Ensure you have Rust installed.

- From the project root run:
````bash
cargo build --release
````

- Add urls in the file sites.txt


- Use this command to run it:
````
cargo run -- --file sites.txt 
````