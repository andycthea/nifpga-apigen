use regex::Regex;
use std::fs::{self, File};
use clap::Parser;
use std::io::prelude::*;

#[derive(Debug)]
struct Item {
    name: String,
    address: String,
    datatype: String,
}

#[derive(Debug)]
struct SizedItem {
    name: String,
    address: String,
    datatype: String,
    size: u32,
}

#[derive(Debug, PartialEq, PartialOrd, Eq, Ord)]
struct GroupElement {
    i: u32,
    address: String,
}

#[derive(Debug)]
struct Group {
    name: String,
    datatype: String,
    elements: Vec<GroupElement>,
}

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
    let mut indicators = Vec::<Item>::new();
    let mut controls = Vec::<Item>::new();
    let mut write_fifos = Vec::<Item>::new();
    let mut read_fifos = Vec::<Item>::new();
    let mut array_indicators = Vec::<SizedItem>::new();
    let mut array_controls = Vec::<SizedItem>::new();
    let mut indicator_groups = Vec::<Group>::new();
    let mut control_groups = Vec::<Group>::new();
    let contents = fs::read_to_string(&input).unwrap();
    for caps in Regex::new(r"NiFpga_.+_(?P<item>Indicator|Control|TargetToHostFifo|HostToTargetFifo)(?P<array>(Array)?)(?P<type>([^_\sS](?:[0-9]+|)|S[^_\si]|Si[^_\sz]|Siz[^_\se])?+)(?P<size>(Size)?)_(?P<name>.+)\s=\s(?P<address>.+)(?:,|)").unwrap().captures_iter(&contents) {
        let item = Item{name: caps["name"].to_string(), address: caps["address"].to_string(), datatype: match &caps["type"]{
            "I8" => "i8",
            "U8" => "u8",
            "I16" => "i16",
            "U16" => "u16",
            "I32" => "i32",
            "U32" => "u32",
            "I64" => "i64",
            "U64" => "u64",
            "Sgl" => "f32",
            "Dbl" => "f64",
            "Bool" => "bool",
            unknown => panic!("unknown type {}", unknown)
        }.to_string()};
        if &caps["array"] == "Array"{
            let arr = if &caps["item"] == "Indicator" {&mut array_indicators} else {&mut array_controls};
            if &caps["size"] == "Size"{
                match arr.iter_mut().position(|el| el.name == item.name){
                    Some(index) => {arr[index].size = item.address.parse().unwrap()},
                    None => {}
                };
            }
            else{
                arr.push(SizedItem{name: item.name, datatype: item.datatype, address: item.address, size: 0})
            }
        }
        else{
            let (arr, group_arr) = match &caps["item"] {
                "Indicator" => (&mut indicators, Some(&mut indicator_groups)),
                "Control" => (&mut controls, Some(&mut control_groups)),
                "TargetToHostFifo" => (&mut read_fifos, None),
                "HostToTargetFifo" => (&mut write_fifos, None),
                unknown => panic!("unknown item {}", unknown)
            };
            match group_arr {
                Some(group_arr) => {
                    match Regex::new(r"(?P<name>.+)_(?P<i>\d+)$").unwrap().captures(&item.name) {
                        Some(caps) => {
                            let element = GroupElement{address: item.address.clone(), i: caps["i"].parse().unwrap()};
                            match group_arr.iter_mut().position(|el| el.name == caps["name"] && el.datatype == item.datatype){
                                Some(index) => {group_arr[index].elements.push(element)},
                                None => {group_arr.push(Group{name: caps["name"].to_string(), datatype: item.datatype.clone(), elements: vec![element]})}
                            }
                        },
                        None => {}
                    }
                }
                None => {}
            }
            arr.push(item);
        }
        
    }
    let cap = Regex::new(r"NiFpga_.+_Signature\s=\s(?P<signature>.+);")
        .unwrap()
        .captures(&contents)
        .unwrap();
    indicator_groups
        .iter_mut()
        .for_each(|group| group.elements.sort_unstable());

    let mut fns = "".to_string();
    for indicator in indicators.iter() {
        fns.push_str(&format!(
            "\tpub fn read_{name}(&self) -> Result<{datatype}, NifpgaError>{{\n\
            \t\tself.session.read::<{datatype}>({address})\n\
            \t}}\n",
            name = indicator.name, datatype = indicator.datatype, address = indicator.address
        ));
    };
    for control in controls.iter(){
        fns.push_str(&format!(
            "\tpub fn write_{name}(&self, value: {datatype}) -> Result<(), NifpgaError>{{\n\
            \t\tself.session.write({address}, value)\n\
            \t}}\n",
            name = control.name, datatype = control.datatype, address = control.address
        ));
    };
    for indicator in array_indicators.iter(){
        fns.push_str(&format!(
            "\tpub fn read_{name}(&self) -> Result<[{datatype}; {size}], NifpgaError>{{\n\
            \t\tlet mut array: [{datatype}; {size}] = Default::default();\n\
            \t\tself.session.read_array::<{datatype}>({address}, &mut array)?;\n\
            \t\tOk(array)\n\
            \t}}\n",
            name = indicator.name, datatype = indicator.datatype, address = indicator.address, size = indicator.size
        ));
    };
    for control in array_controls.iter(){
        fns.push_str(&format!(
            "\tpub fn write_{name}(&self, array: &[{datatype}; {size}]) -> Result<(), NifpgaError>{{\n\
            \t\tself.session.write_array::<{datatype}>({address}, array)\n\
            \t}}\n",
            name = control.name, datatype = control.datatype, address = control.address, size = control.size
        ));
    };
    if groups {
        for group in indicator_groups.iter(){
            fns.push_str(&format!(
                "\tpub fn read_{name}s(&self) -> Result<[{datatype}; {size}], NifpgaError>{{\n\
                \t\tlet mut array: [{datatype}; {size}] = Default::default();\n",
                name = group.name, datatype = group.datatype, size = group.elements.len()
            ));
            for (i, el) in group.elements.iter().enumerate(){
                fns.push_str(&format!(
                    "\t\tarray[{i}] = self.session.read::<{datatype}>({address})?;\n",
                    datatype = group.datatype, address = el.address, i = i
                ));
            };
            fns.push_str("\t\tOk(array)\n\t}\n");
        };
        for group in control_groups.iter(){
            fns.push_str(&format!(
                "\tpub fn write_{name}s(&self, array: &[{datatype}; {size}]) -> Result<(), NifpgaError>{{\n",
                name = group.name, datatype = group.datatype, size = group.elements.len()
            ));
            for (i, el) in group.elements.iter().enumerate(){
                fns.push_str(&format!(
                    "\t\tself.session.write({address}, array[{i}])?;\n",
                    address = el.address, i = i
                ));
            };
            fns.push_str("\t\tOk(())\n\t}\n");
        };
    }
    for fifo in read_fifos.iter(){
        fns.push_str(&format!(
            "\tpub fn open_{name}(&self, depth: usize) -> Result<(ReadFifo<{datatype}>, usize), NifpgaError>{{\n\
            \t\tself.session.open_read_fifo::<{datatype}>({address}, depth)\n\
            \t}}\n",
            name = fifo.name, datatype = fifo.datatype, address = fifo.address
        ));
    };
    for fifo in write_fifos.iter(){
        fns.push_str(&format!(
            "\tpub fn open_{name}(&self, depth: usize) -> Result<(WriteFifo<{datatype}>, usize), NifpgaError>{{\n\
            \t\tself.session.open_write_fifo::<{datatype}>({address}, depth)\n\
            \t}}\n",
            name = fifo.name, datatype = fifo.datatype, address = fifo.address
        ));
    };

    let mut file = File::create(out).unwrap();
    file.write_all(format!(
        "//generated with nifpga-apigen\n\
        use nifpga::{{NifpgaError, Session, ReadFifo, WriteFifo}};\n\
        \n\
        pub struct Fpga {{\n\
        \tpub session: Session\n\
        }}\n\
        \n\
        impl Fpga {{\n\
        \tpub fn open() -> Result<Fpga, NifpgaError>{{\n\
            \t\tOk(Fpga{{session: Session::open(\n\
                \t\t\t\"{path}\",\n\
                \t\t\t{signature},\n\
                \t\t\t\"{resource}\",\n\
                \t\t\t{run},\n\
                \t\t\t{reset_on_close}\n\
            \t\t)?}})\n\
        \t}}\n\
        {fns}}}",
        fns = fns,
        path = path,
        signature = &cap["signature"],
        resource = resource,
        run = run,
        reset_on_close = reset_on_close
    ).as_bytes()).unwrap();
}