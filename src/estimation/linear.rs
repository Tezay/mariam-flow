use crate::state::{OccupancyReading, OccupancyStatus, SensorReading};
use std::time::SystemTime;

/// Distance threshold (mm) below which a sensor is considered occupied.
pub const OCCUPANCY_DISTANCE_MM: u16 = 1200;

pub fn compute_occupancy(readings: &[SensorReading], timestamp: SystemTime) -> OccupancyReading {
    let mut valid_count = 0u32;
    let mut occupied_count = 0u32;
    let mut error_count = 0u32;

    for reading in readings {
        match reading.status {
            crate::state::ReadingStatus::Ok { .. } => {
                valid_count += 1;
                if reading.distance_mm <= OCCUPANCY_DISTANCE_MM {
                    occupied_count += 1;
                }
            }
            crate::state::ReadingStatus::Error { .. } => {
                error_count += 1;
            }
        }
    }

    if valid_count == 0 {
        return OccupancyReading {
            occupancy_percent: None,
            timestamp,
            status: OccupancyStatus::NoData,
        };
    }

    let occupancy_percent =
        ((occupied_count as f64 / valid_count as f64) * 100.0).clamp(0.0, 100.0);
    let status = if error_count == 0 {
        OccupancyStatus::Ok
    } else {
        OccupancyStatus::Degraded
    };

    OccupancyReading {
        occupancy_percent: Some(occupancy_percent),
        timestamp,
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::ReadingStatus;
    use crate::sensor::SensorRangeStatus;
    use std::time::UNIX_EPOCH;

    fn ok_reading(sensor_id: u32, distance_mm: u16) -> SensorReading {
        SensorReading {
            sensor_id,
            distance_mm,
            timestamp: UNIX_EPOCH,
            status: ReadingStatus::Ok {
                range_status: SensorRangeStatus::Valid,
            },
        }
    }

    fn error_reading(sensor_id: u32) -> SensorReading {
        SensorReading {
            sensor_id,
            distance_mm: 0,
            timestamp: UNIX_EPOCH,
            status: ReadingStatus::Error {
                reason: "read failed".to_string(),
            },
        }
    }

    #[test]
    fn occupancy_mixed_valid_and_error_is_degraded() {
        let readings = vec![
            ok_reading(1, 800),
            ok_reading(2, 1500),
            error_reading(3),
        ];

        let occupancy = compute_occupancy(&readings, UNIX_EPOCH);

        assert_eq!(occupancy.occupancy_percent, Some(50.0));
        assert_eq!(occupancy.status, OccupancyStatus::Degraded);
    }

    #[test]
    fn occupancy_all_valid_is_ok() {
        let readings = vec![ok_reading(1, 900), ok_reading(2, 1000)];

        let occupancy = compute_occupancy(&readings, UNIX_EPOCH);

        assert_eq!(occupancy.occupancy_percent, Some(100.0));
        assert_eq!(occupancy.status, OccupancyStatus::Ok);
    }

    #[test]
    fn occupancy_threshold_is_inclusive() {
        let readings = vec![ok_reading(1, OCCUPANCY_DISTANCE_MM)];

        let occupancy = compute_occupancy(&readings, UNIX_EPOCH);

        assert_eq!(occupancy.occupancy_percent, Some(100.0));
        assert_eq!(occupancy.status, OccupancyStatus::Ok);
    }

    #[test]
    fn occupancy_no_data_returns_none() {
        let readings = vec![error_reading(1), error_reading(2)];

        let occupancy = compute_occupancy(&readings, UNIX_EPOCH);

        assert_eq!(occupancy.occupancy_percent, None);
        assert_eq!(occupancy.status, OccupancyStatus::NoData);
    }
}
