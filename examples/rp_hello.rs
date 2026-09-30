//! M0 嵌入冒烟验收锚点：RP 创世纪 + 语言特性 + 冻结 stdlib。
//! 运行：`cargo run -p pygame-rs --example rp_hello`

fn main() {
    pygame_rs::rpy::run_smoke().expect("RP 冒烟失败");
}
