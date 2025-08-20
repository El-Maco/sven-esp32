use serde::Serialize;

use crate::gpio::PulsePin;

#[derive(Debug, Copy, Serialize, Clone, PartialEq, Eq, Hash)]
pub enum SvenPosition {
    Bottom,
    Top,
    Armrest,
    AboveArmrest,
    Standing,
    Custom,
}

impl TryFrom<u32> for SvenPosition {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(SvenPosition::Bottom),
            1 => Ok(SvenPosition::Top),
            2 => Ok(SvenPosition::Armrest),
            3 => Ok(SvenPosition::AboveArmrest),
            4 => Ok(SvenPosition::Standing),
            5 => Ok(SvenPosition::Custom),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct SvenStatePub {
    pub height_mm: u32,
    pub position: SvenPosition,
}

impl SvenStatePub {
    pub fn new(sven_state: &SvenState) -> Self {
        SvenStatePub {
            height_mm: sven_state.height_mm,
            position: sven_state.position,
        }
    }
}

pub struct SvenState<'d> {
    pub height_mm: u32,
    pub position: SvenPosition,
    pin_up: PulsePin<'d>,
    pin_down: PulsePin<'d>,
}

impl<'d> SvenState<'d> {
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
    pub async fn new(pin_up: PulsePin<'d>, pin_down: PulsePin<'d>) -> Self {
        let mut sven_state = SvenState {
            height_mm: 0,
            position: SvenPosition::Custom,
            pin_up,
            pin_down,
        };
        sven_state.move_to_position(SvenPosition::Armrest).await;
        sven_state
    }

    fn get_position_mm(&self, position: SvenPosition) -> u32 {
        Self::POSITIONS_MM
            .iter()
            .find(|&&(pos, _)| pos == position)
            .map_or(Self::MIN_HEIGHT_MM, |&(_, height)| height)
    }

    fn get_duration_mm(&self, ms: u32) -> u32 {
        // handle 11s ->
        let s = ms / 1000;
        if s > 10 {
            // +38 mm for each second above 10s
            return 347 + 38 * (s - 10); // TODO: improve
        }
        Self::MS_TO_CM
            .iter()
            .find(|&&(m, _)| (m / 1000) == (ms / 1000))
            .map_or(0, |&(_, mm)| mm) // Convert cm to mm
    }

    pub async fn move_to_position(&mut self, position: SvenPosition) {
        if self.position == SvenPosition::Custom {
            self.move_up(20000).await; // Move to top position
            self.position = SvenPosition::Top;
            self.height_mm = Self::MAX_HEIGHT_MM;
        }
        match self.position {
            SvenPosition::Top => match position {
                SvenPosition::Top => self.move_up(5000).await, // Move up just in case
                SvenPosition::Standing => self.move_down(4300).await,
                SvenPosition::AboveArmrest => self.move_down(13500).await,
                SvenPosition::Armrest => self.move_down(14800).await,
                SvenPosition::Bottom => self.move_down(20000).await,
                _ => {}
            },
            SvenPosition::Armrest => match position {
                SvenPosition::Bottom => self.move_down(5000).await,
                SvenPosition::AboveArmrest => self.move_up(1920).await,
                SvenPosition::Standing => self.move_up(11000).await,
                SvenPosition::Top => self.move_up(16000).await,
                _ => {}
            },
            SvenPosition::AboveArmrest => match position {
                SvenPosition::Armrest => self.move_down(1900).await,
                SvenPosition::Bottom => self.move_down(7000).await,
                SvenPosition::Standing => self.move_up(9900).await,
                SvenPosition::Top => self.move_up(15000).await,
                _ => {}
            },
            SvenPosition::Standing => match position {
                SvenPosition::Armrest => self.move_down(10800).await,
                SvenPosition::AboveArmrest => self.move_down(9900).await,
                SvenPosition::Bottom => self.move_down(15000).await,
                _ => {}
            },
            _ => {}
        }
        self.position = position;
        self.height_mm = self.get_position_mm(position);
    }

    pub async fn move_up(&mut self, delta_ms: u32) {
        let delta_mm = self.get_duration_mm(delta_ms);

        self.height_mm = Self::MAX_HEIGHT_MM.min(self.height_mm.saturating_add(delta_mm));
        self.pin_up.pulse(delta_ms).await;
    }

    pub async fn move_down(&mut self, delta_ms: u32) {
        let delta_mm = self.get_duration_mm(delta_ms);
        self.height_mm = Self::MIN_HEIGHT_MM.max(self.height_mm.saturating_sub(delta_mm));
        self.pin_down.pulse(delta_ms).await;
    }

    pub async fn move_up_relative(&mut self, delta_mm: u32) {
        let mut distance_left = delta_mm;
        while distance_left > 0 {
            // find the duration of the maximum distance that fits into the dinstance_left
            let (max_duration, max_distance) = Self::MS_TO_CM
                .iter()
                .rev()
                .find(|&&(_, mm)| mm <= distance_left)
                .unwrap_or(&(0, 0));
            if *max_duration == 0 {
                break; // No more distance can be moved (within 9 mm)
            }
            self.move_up(*max_duration).await;
            distance_left = distance_left.saturating_sub(*max_distance);
        }
    }

    pub async fn move_down_relative(&mut self, delta_mm: u32) {
        let mut distance_left = delta_mm;
        while distance_left > 0 {
            // find the duration of the maximum distance that fits into the distance_left
            let (max_duration, max_distance) = Self::MS_TO_CM
                .iter()
                .rev()
                .find(|&&(_, mm)| mm <= distance_left)
                .unwrap_or(&(0, 0));
            if *max_duration == 0 {
                break; // No more distance can be moved (within 9 mm)
            }
            self.move_down(*max_duration).await;
            distance_left = distance_left.saturating_sub(*max_distance);
        }
    }

    pub async fn move_to_height(&mut self, height_mm: u32) {
        if height_mm == self.height_mm {
            return; // Already at the desired height
        }

        if height_mm < Self::MIN_HEIGHT_MM {
            self.move_to_position(SvenPosition::Bottom).await;
            return;
        }
        if height_mm > Self::MAX_HEIGHT_MM {
            self.move_to_position(SvenPosition::Top).await;
            return; // Invalid height
        }

        if height_mm > self.height_mm {
            let delta_mm = height_mm - self.height_mm;
            self.move_up_relative(delta_mm).await;
        } else {
            let delta_mm = self.height_mm - height_mm;
            self.move_down_relative(delta_mm).await;
        }
    }
}
