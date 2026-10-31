use clap::{Parser, Subcommand};

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
    let name = match cli.command {
        Command::Migrate => "migrate",
        Command::Worker => "worker",
        Command::Web => "web",
        Command::Healthcheck { .. } => "healthcheck",
    };
    eprintln!("{name}: 아직 구현하지 않았습니다(R0 뼈대).");
    std::process::exit(2);
}
