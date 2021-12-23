use clap::Parser;

#[derive(Parser, Debug)]
/// Lightweight Process-Killer Daemon for Out-of-Memory Scenarios
#[clap(author, version, about)]
pub struct Cli {
    /// Log extra debugging information
    #[clap(short, long)]
    pub verbose: bool,
    /// Do not fork a daemon process
    #[clap(short, long)]
    pub no_daemonize: bool,
    /// Kill the entire process group of a target
    #[clap(short, long)]
    pub kill_pgroup: bool,
    /// The cieling PSI value; All processes that surpass this will be killed
    #[clap(short = 'p', long, value_name = "PSI", default_value = "25.0")]
    pub cutoff_psi: f32,
    // #[cfg(feature = "glob-ignore")]
    /// A (tilde-separated) list of glob patterns matching process names which will never be killed
    #[clap(short = 'u', long = "unkillable", value_name = "GLOB")]
    pub ignored_globs: Option<Vec<String>>,
}
