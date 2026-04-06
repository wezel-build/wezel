use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::wezel_dir;

const SOURCE_MARKER: &str = "# >>> wezel pheromone >>>";
const SOURCE_END: &str = "# <<< wezel pheromone <<<";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    pub fn detect() -> Option<Self> {
        let shell = std::env::var("SHELL").ok()?;
        if shell.contains("zsh") {
            Some(Shell::Zsh)
        } else if shell.contains("bash") {
            Some(Shell::Bash)
        } else if shell.contains("fish") {
            Some(Shell::Fish)
        } else {
            None
        }
    }

    pub fn rc_paths(self) -> Vec<PathBuf> {
        let home = dirs::home_dir().expect("could not determine home directory");
        match self {
            Shell::Zsh => vec![home.join(".zshenv"), home.join(".zshrc")],
            Shell::Bash => {
                let bashrc = home.join(".bashrc");
                if bashrc.exists() {
                    vec![bashrc]
                } else {
                    vec![home.join(".bash_profile")]
                }
            }
            Shell::Fish => vec![home.join(".config/fish/conf.d/wezel.fish")],
        }
    }

    pub fn init_script_path(self) -> PathBuf {
        let dir = wezel_dir();
        match self {
            Shell::Zsh => dir.join("init.zsh"),
            Shell::Bash => dir.join("init.bash"),
            Shell::Fish => dir.join("init.fish"),
        }
    }

    pub fn source_block(self) -> String {
        let path = self.init_script_path();
        let p = path.display();
        match self {
            Shell::Zsh | Shell::Bash => {
                format!("{SOURCE_MARKER}\n[[ -f \"{p}\" ]] && source \"{p}\"\n{SOURCE_END}")
            }
            Shell::Fish => {
                format!(
                    "{SOURCE_MARKER}\nif test -f \"{p}\"\n    source \"{p}\"\nend\n{SOURCE_END}"
                )
            }
        }
    }

    pub fn alias_line(self, name: &str, handler: &str) -> String {
        match self {
            Shell::Zsh | Shell::Bash => {
                format!("{name}() {{ wezel exec -- {handler} \"$@\"; }}")
            }
            Shell::Fish => {
                format!("function {name}; wezel exec -- {handler} $argv; end")
            }
        }
    }

    fn completion_eval(self, exe: &str) -> String {
        match self {
            Shell::Zsh => format!("eval \"$(COMPLETE=zsh \"{exe}\" 2>/dev/null)\""),
            Shell::Bash => format!("eval \"$(COMPLETE=bash \"{exe}\" 2>/dev/null)\""),
            Shell::Fish => format!("COMPLETE=fish \"{exe}\" 2>/dev/null | source"),
        }
    }

    pub fn render_init_script(self, aliases: &BTreeMap<String, String>) -> String {
        let exe = std::env::current_exe()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "wezel".to_string());
        let mut out =
            String::from("# Managed by wezel — do not edit, aliases are stored in aliases.toml\n");
        for (name, handler) in aliases {
            out.push_str(&self.alias_line(name, handler));
            out.push('\n');
        }
        out.push_str(&self.completion_eval(&exe));
        out.push('\n');
        out
    }
}

pub fn ensure_shell_hook(shell: Shell) -> anyhow::Result<()> {
    for rc in shell.rc_paths() {
        if shell == Shell::Fish
            && let Some(parent) = rc.parent()
        {
            fs::create_dir_all(parent)?;
        }

        let contents = if rc.exists() {
            fs::read_to_string(&rc)?
        } else {
            String::new()
        };

        if contents.contains(SOURCE_MARKER) {
            continue;
        }

        let mut file = fs::OpenOptions::new().create(true).append(true).open(&rc)?;

        writeln!(file)?;
        writeln!(file, "{}", shell.source_block())?;

        println!("Installed source hook in {}", rc.display());
    }
    Ok(())
}

pub fn sync_init_script(shell: Shell, aliases: &BTreeMap<String, String>) -> anyhow::Result<()> {
    let dir = wezel_dir();
    fs::create_dir_all(&dir)?;
    let script = shell.render_init_script(aliases);
    fs::write(shell.init_script_path(), script)?;
    Ok(())
}
