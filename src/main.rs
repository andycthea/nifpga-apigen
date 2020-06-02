use regex::Regex;
use std::fs::{self, File};
use clap::{Arg, App};
use std::path::PathBuf;
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

fn main() {
    
    let matches = App::new("nifpga-apigen")
        .version("0.1.0")
        .arg(Arg::with_name("input")
             .help("Sets the input file to use")
             .required(true)
             .index(1))
        .arg(Arg::with_name("out")
            .short("o")
            .long("out")
            .takes_value(true)
            .help("output file relative to input directory. defaults to mod.rs"))
        .arg(Arg::with_name("path")
            .short("p")
            .long("path")
            .takes_value(true)
            .help("bitfile path. defaults to /home/lvuser/fpga.lvbitx"))
        .arg(Arg::with_name("resource")
            .short("r")
            .long("res")
            .takes_value(true)
            .help("resource name. defaults to RIO0"))
        .arg(Arg::with_name("no-run")
            .long("no-run")
            .help("if present, the bitfile will not run when opened"))
        .arg(Arg::with_name("no-reset")
            .long("no-reset")
            .short("n")
            .help("if present, the bitfile will not reset when closed"))
        .arg(Arg::with_name("groups")
            .long("groups")
            .short("g")
            .help("if present, enumerated controls and indicators will have batch access methods"))
        .get_matches();
    let input = matches.value_of("input").unwrap();
    let mut out: PathBuf = input.parse().unwrap();
    out.pop();
    out.push(matches.value_of("out").unwrap_or("mod.rs"));
    let path = matches.value_of("path").unwrap_or("/home/lvuser/fpga.lvbitx");
    let resource = matches.value_of("resource").unwrap_or("RIO0");
    let run= !matches.is_present("no-run");
    let reset_on_close = !matches.is_present("no-reset");
    let groups = !matches.is_present("groups");
    let mut indicators = Vec::<Item>::new();
    let mut controls = Vec::<Item>::new();
    let mut write_fifos = Vec::<Item>::new();
    let mut read_fifos = Vec::<Item>::new();
    let mut array_indicators = Vec::<SizedItem>::new();
    let mut array_controls = Vec::<SizedItem>::new();
    let mut indicator_groups = Vec::<Group>::new();
    let mut control_groups = Vec::<Group>::new();
    let contents = fs::read_to_string(&input).unwrap();
    for caps in Regex::new(r"NiFpga_.+_(?P<item>Indicator|Control|TargetToHostFifo|HostToTargetFifo)(?P<array>(Array)?)(?P<type>([^_\sS]|S[^_\si]|Si[^_\sz]|Siz[^_\se])?+)(?P<size>(Size)?)_(?P<name>.+)\s=\s(?P<address>.+),").unwrap().captures_iter(&contents) {
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

    let mut trait_fns = "".to_string();
    let mut impl_fns = "".to_string();
    indicators.iter().for_each(|indicator| {
        trait_fns.push_str(&format!(
            "\tfn read_{}(&self) -> Result<{}, NifpgaError>;\n",
            indicator.name, indicator.datatype
        ));
        impl_fns.push_str(&format!(
            "\tfn read_{name}(&self) -> Result<{datatype}, NifpgaError>{{\n\
            \t\tself.read::<{datatype}>({address})\n\
            \t}}\n",
            name = indicator.name, datatype = indicator.datatype, address = indicator.address
        ));
    });
    controls.iter().for_each(|control| {
        trait_fns.push_str(&format!(
            "\tfn write_{}(&self, value: {}) -> Result<(), NifpgaError>;\n",
            control.name, control.datatype
        ));
        impl_fns.push_str(&format!(
            "\tfn write_{name}(&self, value: {datatype}) -> Result<(), NifpgaError>{{\n\
            \t\tself.write({address}, value)\n\
            \t}}\n",
            name = control.name, datatype = control.datatype, address = control.address
        ));
    });
    array_indicators.iter().for_each(|indicator| {
        trait_fns.push_str(&format!(
            "\tfn read_{}(&self) -> Result<[{}; {}], NifpgaError>;\n",
            indicator.name, indicator.datatype, indicator.size
        ));
        impl_fns.push_str(&format!(
            "\tfn read_{name}(&self) -> Result<[{datatype}; {size}], NifpgaError>{{\n\
            \t\tlet mut array: [{datatype}; {size}] = Default::default();\n\
            \t\tself.read_array::<{datatype}>({address}, &mut array)?;\n\
            \t\tOk(array)\n\
            \t}}\n",
            name = indicator.name, datatype = indicator.datatype, address = indicator.address, size = indicator.size
        ));
    });
    array_controls.iter().for_each(|control| {
        trait_fns.push_str(&format!(
            "\tfn write_{}(&self, array: &[{}; {}]) -> Result<(), NifpgaError>;\n",
            control.name, control.datatype, control.size
        ));
        impl_fns.push_str(&format!(
            "\tfn write_{name}(&self, array: &[{datatype}; {size}]) -> Result<(), NifpgaError>{{\n\
            \t\tself.write_array::<{datatype}>({address}, array)\n\
            \t}}\n",
            name = control.name, datatype = control.datatype, address = control.address, size = control.size
        ));
    });
    if groups {
        indicator_groups.iter().for_each(|group| {
            trait_fns.push_str(&format!(
                "\tfn read_{}s(&self) -> Result<[{}; {}], NifpgaError>;\n",
                group.name, group.datatype, group.elements.len()
            ));
            impl_fns.push_str(&format!(
                "\tfn read_{name}s(&self) -> Result<[{datatype}; {size}], NifpgaError>{{\n\
                \t\tlet mut array: [{datatype}; {size}] = Default::default();\n",
                name = group.name, datatype = group.datatype, size = group.elements.len()
            ));
            group.elements.iter().enumerate().for_each(|(i, el)| {
                impl_fns.push_str(&format!(
                    "\t\tarray[{i}] = self.read::<{datatype}>({address})?;\n",
                    datatype = group.datatype, address = el.address, i = i
                ));
            });
            impl_fns.push_str("\t\tOk(array)\n\t}\n");
        });
        control_groups.iter().for_each(|group| {
            trait_fns.push_str(&format!(
                "\tfn write_{}s(&self, array: &[{}; {}]) -> Result<(), NifpgaError>;\n",
                group.name, group.datatype, group.elements.len()
            ));
            impl_fns.push_str(&format!(
                "\tfn write_{name}s(&self, array: &[{datatype}; {size}]) -> Result<(), NifpgaError>{{\n",
                name = group.name, datatype = group.datatype, size = group.elements.len()
            ));
            group.elements.iter().enumerate().for_each(|(i, el)| {
                impl_fns.push_str(&format!(
                    "\t\tself.write({address}, array[{i}])?;\n",
                    address = el.address, i = i
                ));
            });
            impl_fns.push_str("\t\tOk(())\n\t}\n");
        });
    }
    read_fifos.iter().for_each(|fifo| {
        trait_fns.push_str(&format!(
            "\tfn open_{}(&self, depth: usize) -> Result<(ReadFifo<{}>, usize), NifpgaError>;\n",
            fifo.name, fifo.datatype
        ));
        impl_fns.push_str(&format!(
            "\tfn open_{name}(&self, depth: usize) -> Result<(ReadFifo<{datatype}>, usize), NifpgaError>{{\n\
            \t\tself.open_read_fifo::<{datatype}>({address}, depth)\n\
            \t}}\n",
            name = fifo.name, datatype = fifo.datatype, address = fifo.address
        ));
    });
    write_fifos.iter().for_each(|fifo| {
        trait_fns.push_str(&format!(
            "\tfn open_{}(&self, depth: usize) -> Result<(WriteFifo<{}>, usize), NifpgaError>;\n",
            fifo.name, fifo.datatype
        ));
        impl_fns.push_str(&format!(
            "\tfn open_{name}(&self, depth: usize) -> Result<(WriteFifo<{datatype}>, usize), NifpgaError>{{\n\
            \t\tself.open_write_fifo::<{datatype}>({address}, depth)\n\
            \t}}\n",
            name = fifo.name, datatype = fifo.datatype, address = fifo.address
        ));
    });

    let mut file = File::create(out).unwrap();
    file.write_all(format!(
        "//generated with nifpga-apigen\n\
        use nifpga::{{NifpgaError, Session, ReadFifo, WriteFifo}};\n\
        \n\
        pub trait Fpga {{\n\
        {trait_fns}}}\n\
        \n\
        impl Fpga for Session {{\n\
        {impl_fns}}}\n\
        \n\
        pub fn open() -> Result<Session, NifpgaError>{{\n\
            \tSession::open(\n\
                \t\t\"{path}\",\n\
                \t\t{signature},\n\
                \t\t\"{resource}\",\n\
                \t\t{run},\n\
                \t\t{reset_on_close}\n\
            \t)\n\
        }}",
        trait_fns = trait_fns,
        impl_fns = impl_fns,
        path = path,
        signature = &cap["signature"],
        resource = resource,
        run = run,
        reset_on_close = reset_on_close
    ).as_bytes()).unwrap();
}