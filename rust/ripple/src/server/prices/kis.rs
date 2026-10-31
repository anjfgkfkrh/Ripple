// R8에서 KIS 클라이언트를 옮긴다. R1(worker 설정)에는 종류만 필요하다.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KisEnv {
    Demo,
    Real,
}
