use clap::Parser;
use nifpga_apigen::generate;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Sets the input file to use
    #[arg(short, long)]
    input: String,

    /// output file relative to input directory. defaults to mod.rs
    #[arg(short, long, default_value = "mod.rs")]
    out: String,

    /// bitfile path. defaults to /home/lvuser/fpga.lvbitx
    #[arg(short, long, default_value = "/home/lvuser/fpga.lvbitx")]
    path: String,

    /// resource name. defaults to RIO0
    #[arg(short, long, default_value = "RIO0")]
    resource: String,

    /// if present, the bitfile will not run when opened
    #[arg(long)]
    no_run: bool,

    /// if present, the bitfile will not reset when closed
    #[arg(long)]
    no_reset: bool,

    /// if present, enumerated controls and indicators will have batch access methods
    #[arg(long)]
    groups: bool,
}

fn main() {
    let args = Args::parse();
    
    let input = args.input;
    let out = args.out;
    let path = args.path;
    let resource = args.resource;
    let run = !args.no_run;
    let reset_on_close = !args.no_reset;
    let groups = args.groups;
    match generate(&input, &out, &path, &resource, run, reset_on_close, groups) {
        Ok(_) => println!("generated {}", out),
        Err(e) => eprintln!("{}", e)
    }
}