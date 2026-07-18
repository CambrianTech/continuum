use clap::{App, Arg};

fn main() {
    let matches = App::new("wordstats")
        .version("1.0")
        .author("Asha <asha@example.com>")
        .about("Calculates word frequencies in a text file")
        .arg(
            Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("FILE")
                .help("Sets the input file to use")
                .takes_value(true),
        )
        .get_matches();

    let input_file = matches.value_of("input").unwrap_or("sample.txt");
    // Continue with processing the input file...
}