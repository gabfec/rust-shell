#[allow(unused_imports)]
use std::io::{self, Write};

const SHELL_BUILTINS: [&str; 3] = ["exit", "echo", "type"];

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
            "type" => if SHELL_BUILTINS.contains(&cmd[1])  {
                println!("{} is a shell builtin", cmd[1]);
            } else {
                println!("{}: not found", cmd[1]);
            },
            _ => println!("{}: command not found", command.trim()),
        }
    }
}
