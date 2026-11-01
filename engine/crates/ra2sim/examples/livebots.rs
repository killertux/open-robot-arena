//! `livebots <game> <out dir>`: the robots the port's lab writes, for loading in the original
//! (docs/36 9.27f): a triangle chassis with a dragged top point and a built bot with a motor, a
//! wheel and the default control board.
use ra2ui::workshop::build::{Build, Placement};
use ra2ui::workshop::chassis::Outline;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (game, out) = (
        std::path::PathBuf::from(&a[1]),
        std::path::PathBuf::from(&a[2]),
    );
    std::env::set_var("RA2_GAME", &game);
    std::fs::create_dir_all(&out).unwrap();

    let mut tri = Build::blank("OraTriangle");
    for p in [[200, 200], [800, 200], [500, 800]] {
        tri.add_chassis_outline_point(Outline::Bottom, p, true)
            .unwrap();
    }
    tri.begin_top_adjust().unwrap();
    tri.adjust_top_point(0, [400, 300]).unwrap();
    tri.save_to(&out.join("OraTriangle.bot")).unwrap();

    let library = ra2ui::workshop::library::GameLibrary::open(&game).unwrap();
    let stock =
        ra2ui::workshop::library::GameLibrary::read_bot(&game.join("AI/Team1/Bot0.bot")).unwrap();
    let mut chassis = stock.components[0].chassis.clone().unwrap();
    chassis.texture_tga = None;
    let mut build = Build::new("OraBuilt", chassis).with_default_board();
    let motor_info = library.component_info_by_file("Components\\anglemotor1.txt");
    let base_mount = *motor_info.mounts.keys().next().unwrap();
    let motor = build
        .attach(
            &library,
            "Components\\anglemotor1.txt",
            0,
            0,
            base_mount,
            Placement::default(),
        )
        .unwrap();
    let wheel_info = library.component_info_by_file("Components\\miniwheel.txt");
    let own_mount = *wheel_info
        .mounts
        .iter()
        .find(|(_, m)| m.types.iter().any(|t| t == "axle_f"))
        .map(|(id, _)| id)
        .unwrap();
    let axle = *motor_info
        .mounts
        .iter()
        .find(|(_, m)| m.types.iter().any(|t| t == "axle_m"))
        .map(|(id, _)| id)
        .unwrap();
    build
        .attach(
            &library,
            "Components\\miniwheel.txt",
            motor,
            axle,
            own_mount,
            Placement::default(),
        )
        .unwrap();
    build.save_to(&out.join("OraBuilt.bot")).unwrap();
}
