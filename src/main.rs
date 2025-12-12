#[allow(unused_imports)]
use std::io::{self, Write};

fn main() {
    while true {
        print!("$ ");
        io::stdout().flush().unwrap();

        // Wait for user input
        let mut command = String::new();
        io::stdin().read_line(&mut command).unwrap();
        let cmd = command.trim().split(' ').collect::<Vec<&str>>();
        let args = cmd[1..].to_vec();
        match cmd[0] {
            "exit" => break,
            "echo" => println!("{}", args.join(" ") ),
            _ => println!("{}: command not found", command.trim()),
        }
    }
}
