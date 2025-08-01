// this is something like go's flag package, but in rust style

// Represents parsed flag:
//     -name value => { name, Some(value) }
//     -name => { name, None }
struct Flag {
    name:  String,
    value: Option<String>
}

struct FlagDef {
    name:    &'static str,
    desc:    &'static str,
    def_val: FlagValue
}

enum FlagValue {
    Bool(bool),
    Str(&'static str)
}

pub struct Flags {
    parsed:  Vec<Flag>,
    defined: Vec<FlagDef> // information about flags that used for printing help
}

impl Flags {
    pub fn parse() -> Option<Self> {
        let mut res = Self { parsed: Vec::new(), defined: Vec::new() };
        let mut args = std::env::args().skip(1).peekable();
        while let Some(mut arg) = args.next() {
            if !arg.starts_with("-") {
                eprintln!("error: flag name expected, but found '{arg}'");
                return None;
            }

            arg.remove(0); // Remove '-' prefix
            let mut flag = Flag { name: arg, value: None };
            if let Some(v) = args.peek() {
                if !v.starts_with("-") {
                    flag.value = Some(args.next().unwrap());
                }
            }

            res.parsed.push(flag);
        }

        Some(res)
    }

    pub fn flag_bool(&mut self, name: &'static str, desc: &'static str, def: bool) -> Option<bool> {
        self.defined.push(FlagDef {
            name, desc,
            def_val: FlagValue::Bool(def)
        });

        for i in 0..self.parsed.len() {
            if self.parsed[i].name == name {
                let flag = self.parsed.remove(i);
                match flag.value {
                    None => { return Some(true) },
                    Some(_) => {
                        eprintln!("error: flag '-{name}' does not take a value");
                        return None;
                    }
                }
            }
        }

        Some(def)
    }

    pub fn flag_str(&mut self, name: &'static str, desc: &'static str, def: &'static str) -> Option<String> {
        self.defined.push(FlagDef {
            name, desc,
            def_val: FlagValue::Str(def)
        });

        for i in 0..self.parsed.len() {
            if self.parsed[i].name == name {
                let flag = self.parsed.remove(i);
                if flag.value.is_none() {
                    eprintln!("error: flag '-{name}' expects a value");
                    return None;
                }

                return flag.value;
            }
        }

        Some(def.to_string())
    }

    pub fn check(&self) -> Option<()> {
        if !self.parsed.is_empty() {
            for flag in &self.parsed {
                eprintln!("error: flag '-{}' not found", flag.name);
            }

            self.print_flags();

            None
        } else {
            Some(())
        }
    }

    pub fn print_flags(&self) {
        eprintln!("available flags:");
        for FlagDef { name, desc, def_val } in &self.defined {
            match def_val {
                FlagValue::Str(val) => {
                    eprintln!("    -{name} <str> - {desc} (default: \"{val}\")");
                },
                FlagValue::Bool(val) => {
                    eprintln!("    -{name} - {desc} (default: {val})");
                }
            }
        }
    }
}
