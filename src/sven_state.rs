#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum SvenPosition {
    Bottom,
    Top,
    Armrest,
    AboveArmrest,
    Standing,
    Custom,
}

pub struct SvenState {
    height_mm: u32,
    position: SvenPosition,
}

impl SvenState {
    const MIN_HEIGHT_MM: u32 = 622;
    const MAX_HEIGHT_MM: u32 = 1274;
    const POSITIONS_MM: &'static [(SvenPosition, u32)] = &[
        (SvenPosition::Bottom, Self::MIN_HEIGHT_MM),
        (SvenPosition::Armrest, 750),
        (SvenPosition::AboveArmrest, 795),
        (SvenPosition::Standing, 1140),
        (SvenPosition::Top, Self::MAX_HEIGHT_MM),
    ];

    const MS_TO_CM: &'static [(u32, u32)] = &[
        (1000, 9),
        (2000, 48),
        (3000, 82),
        (4000, 119),
        (5000, 160),
        (6000, 194),
        (7000, 234),
        (8000, 272),
        (9000, 310),
        (10000, 347),
    ];

    // Create a new SvenState instance with default position
    // and height set to the armrest position.
    pub fn new() -> Self {
        let mut sven_state = SvenState {
            height_mm: 0,
            position: SvenPosition::Custom,
        };
        sven_state.move_to_position(SvenPosition::Armrest);
        sven_state
    }

    fn get_position_mm(&self, position: SvenPosition) -> u32 {
        Self::POSITIONS_MM
            .iter()
            .find(|&&(pos, _)| pos == position)
            .map_or(Self::MIN_HEIGHT_MM, |&(_, height)| height)
    }

    fn get_duration_mm(&self, ms: u32) -> u32 {
        Self::MS_TO_CM
            .iter()
            .find(|&&(m, _)| m == ms)
            .map_or(0, |&(_, cm)| cm * 10) // Convert cm to mm
    }

    pub fn move_to_position(&mut self, position: SvenPosition) {
        match self.position {
            SvenPosition::Top => match position {
                SvenPosition::Top => self.move_up(5000), // Move up just in case
                SvenPosition::Standing => self.move_down(4300),
                SvenPosition::AboveArmrest => self.move_down(13500),
                SvenPosition::Armrest => self.move_down(14800),
                SvenPosition::Bottom => self.move_down(20000),
                _ => {}
            },
            SvenPosition::Armrest => match position {
                SvenPosition::Bottom => self.move_down(5000),
                SvenPosition::AboveArmrest => self.move_up(1920),
                SvenPosition::Standing => self.move_up(11000),
                SvenPosition::Top => self.move_up(16000),
                _ => {}
            },
            SvenPosition::AboveArmrest => match position {
                SvenPosition::Armrest => self.move_down(1900),
                SvenPosition::Bottom => self.move_down(7000),
                SvenPosition::Standing => self.move_up(9900),
                SvenPosition::Top => self.move_up(15000),
                _ => {}
            },
            SvenPosition::Standing => match position {
                SvenPosition::Armrest => self.move_down(10800),
                SvenPosition::AboveArmrest => self.move_down(9900),
                SvenPosition::Bottom => self.move_down(15000),
                _ => {}
            },
            SvenPosition::Custom => {
                self.move_up(20000);
                self.position = SvenPosition::Top;
                self.move_to_position(position);
            } // Custom position handling can be added later
            _ => {}
        }
        self.position = position;
        self.height_mm = self.get_position_mm(position);
    }

    pub fn move_up(&mut self, delta_ms: u32) {
        let delta_mm = self.get_duration_mm(delta_ms);

        self.height_mm = Self::MAX_HEIGHT_MM
            .min(self.height_mm.saturating_add(delta_mm));
    }

    pub fn move_down(&mut self, delta_ms: u32) {
        let delta_mm = self.get_duration_mm(delta_ms);
        self.height_mm = Self::MIN_HEIGHT_MM.max(self.height_mm.saturating_sub(delta_mm));
    }
}
