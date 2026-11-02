use clap::{Parser, Subcommand};
use ripple::server::env::Env;

#[derive(Parser)]
#[command(
    name = "ripple",
    version,
    about = "Ripple 실행 파일(하위 명령으로 역할을 고른다)"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// DB 마이그레이션을 적용한다
    Migrate,
    /// 수집·평가 worker와 내부 API를 실행한다
    Worker,
    /// 화면(web)을 실행한다
    Web,
    /// 주소를 GET해서 2xx면 종료 코드 0, 아니면 1로 끝난다
    Healthcheck {
        /// 확인할 주소
        url: String,
    },
}

fn main() {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Migrate => ripple::worker::migrate::run(&Env::from_process()),
        Command::Worker => ripple::worker::index::run(&Env::from_process()),
        Command::Web => not_implemented("web"),
        Command::Healthcheck { url } => ripple::healthcheck::run(&url),
    };
    std::process::exit(code);
}

fn not_implemented(name: &str) -> i32 {
    eprintln!("{name}: 아직 구현하지 않았습니다(Rust 전환 중).");
    2
}
