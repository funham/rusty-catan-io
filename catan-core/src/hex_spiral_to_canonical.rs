use catan_core::topology::hex::HexIndex;

fn main() {
    println!("Hex spiral to canonical index");
    loop {
        let mut input = String::new();

        println!("Enter a number:");

        std::io::stdin()
            .read_line(&mut input)
            .expect("Failed to read line");

        let spiral: usize = match input.trim().parse() {
            Ok(num) => num,
            Err(_) => {
                println!("Please enter a valid positive integer.");
                continue;
            }
        };

        println!("{:?}", HexIndex::spiral_to_hex(spiral));
    }
}
