use indexmap::IndexMap;
use regex::Regex;

pub fn remap_enum_types(
    text: String,
    enum_remapping: &IndexMap<String, IndexMap<String, IndexMap<String, String>>>,
) -> String {
    let mut lines: Vec<String> = text.split('\n').map(|s| s.to_string()).collect();

    let regex = Regex::new(r"\.(\w+)\(([\w\[\]:]+)\)").unwrap();

    let mut current_mod_def_name: Option<String> = None;
    let mut current_mod_inst_name: Option<String> = None;

    for line in lines.iter_mut() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("endmodule") {
            current_mod_def_name = None;
            current_mod_inst_name = None;
        } else if trimmed_line.starts_with("module") {
            if let Some(name) = trimmed_line.split_whitespace().nth(1) {
                let def_name = name.split('(').next().unwrap().to_string();
                current_mod_def_name = Some(def_name);
            }
            current_mod_inst_name = None;
        } else if let Some(ref def_name) = current_mod_def_name {
            if let Some(map_of_insts) = enum_remapping.get(def_name) {
                if trimmed_line.ends_with(");") {
                    current_mod_inst_name = None;
                } else if trimmed_line.ends_with("(") {
                    if let Some(inst_name) = trimmed_line.split_whitespace().nth(1) {
                        current_mod_inst_name = Some(inst_name.to_string());
                    }
                } else if let Some(ref inst_name) = current_mod_inst_name {
                    if let Some(map_of_ports) = map_of_insts.get(inst_name) {
                        if trimmed_line.starts_with(".") {
                            let mut tokens = trimmed_line.split('(');
                            let port_name =
                                tokens.next().unwrap().trim_start_matches('.').to_string();
                            if let Some(enum_name) = map_of_ports.get(&port_name) {
                                *line = regex
                                    .replace(line, |caps: &regex::Captures| {
                                        format!(".{}({}'({}))", &caps[1], enum_name, &caps[2])
                                    })
                                    .to_string();
                            }
                        }
                    }
                }
            }
        }
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    #[test]
    fn test_remap_enum_types() {
        let mut enum_remapping = IndexMap::new();

        enum_remapping.insert("ModA".to_string(), IndexMap::new());
        enum_remapping["ModA"].insert("instA".to_string(), IndexMap::new());
        enum_remapping["ModA"]["instA"].insert("portA".to_string(), "EnumTypeA".to_string());

        enum_remapping.insert("ModB".to_string(), IndexMap::new());
        enum_remapping["ModB"].insert("instB".to_string(), IndexMap::new());
        enum_remapping["ModB"]["instB"].insert("portB".to_string(), "EnumTypeB".to_string());

        let input_verilog = "
module ModA (
    input wire [1:0] portA,
    output wire [1:0] portB
);
    wire [1:0] signalA;
    wire [1:0] signalB;
    ModB instA (
        .portA(signalA[1:0]),
        .portB(signalB[1:0])
    );
endmodule

module ModB (
    input wire [1:0] portA,
    output wire [1:0] portB
);
    wire [1:0] signalC;
    wire [1:0] signalD;
    ModC instB (
        .portA(signalC),
        .portB(signalD)
    );
endmodule
"
        .to_string();

        let expected_output = "
module ModA (
    input wire [1:0] portA,
    output wire [1:0] portB
);
    wire [1:0] signalA;
    wire [1:0] signalB;
    ModB instA (
        .portA(EnumTypeA'(signalA[1:0])),
        .portB(signalB[1:0])
    );
endmodule

module ModB (
    input wire [1:0] portA,
    output wire [1:0] portB
);
    wire [1:0] signalC;
    wire [1:0] signalD;
    ModC instB (
        .portA(signalC),
        .portB(EnumTypeB'(signalD))
    );
endmodule
"
        .to_string();

        let result = remap_enum_types(input_verilog, &enum_remapping);
        assert_eq!(result, expected_output);
    }
}
