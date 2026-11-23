pub mod client {
    pub mod camera {
        pub struct CameraEntity;
        pub struct FovPlayer;
    }
    pub mod first_person {
        #[derive(Clone, Copy)]
        pub struct Hand;
    }
    pub mod widgets {
        pub struct TextEdit;
    }
}
