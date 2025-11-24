// Example demonstrating CallTree display formats

use piecrust_uplink::ContractId;

// We need to access internal types for this example
// In real code, you'd use the public API
fn main() {
    println!("CallTree Display Formats Example");
    println!("=================================\n");

    // Create some contract IDs for demonstration
    let id1 = ContractId::from_bytes([0x01; 32]);
    let id2 = ContractId::from_bytes([0x02; 32]);
    let id3 = ContractId::from_bytes([0x03; 32]);
    let id4 = ContractId::from_bytes([0x04; 32]);
    let id5 = ContractId::from_bytes([0x05; 32]);

    println!("Contract IDs:");
    println!("  id1: {}", hex::encode(&id1.to_bytes()[..4]));
    println!("  id2: {}", hex::encode(&id2.to_bytes()[..4]));
    println!("  id3: {}", hex::encode(&id3.to_bytes()[..4]));
    println!("  id4: {}", hex::encode(&id4.to_bytes()[..4]));
    println!("  id5: {}", hex::encode(&id5.to_bytes()[..4]));
    println!();

    // Note: This is just a demonstration of the format
    // In actual usage, the CallTree is created and managed internally
    println!("Display Format (println!(\"{{}}\", tree)):");
    println!("  Compact: 0x01010101[0x02020202[0x04040404], 0x03030303[*0x05050505]]");
    println!();

    println!("Debug Format (println!(\"{{:#?}}\", tree)):");
    println!("  0x01010101");
    println!("  ├── 0x02020202");
    println!("  │   └── 0x04040404");
    println!("  └── 0x03030303");
    println!("      └── *0x05050505");
    println!();

    println!("The * marker indicates the current cursor position in the tree.");
}
