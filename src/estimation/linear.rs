use crate::state::{
    CalibrationParams, OccupancyReading, OccupancyStatus, SensorReading, WaitTimeErrorCode,
    WaitTimeEstimate, WaitTimeStatus,
};
use std::time::SystemTime;

/// Distance threshold (mm) below which a sensor is considered occupied.
pub const OCCUPANCY_DISTANCE_MM: u16 = 1200;
pub const DEFAULT_WAIT_TIME_MINUTES_AT_EMPTY: f64 = 0.0;
pub const DEFAULT_WAIT_TIME_MINUTES_AT_FULL: f64 = 20.0;

pub fn compute_wait_time(
    occupancy: &OccupancyReading,
    timestamp: SystemTime,
    calibration: Option<&CalibrationParams>,
) -> WaitTimeEstimate {
    if occupancy.occupancy_percent.is_none() || matches!(occupancy.status, OccupancyStatus::NoData)
    {
        return WaitTimeEstimate {
            wait_time_minutes: None,
            timestamp,
            status: WaitTimeStatus::Degraded,
            error_code: Some(WaitTimeErrorCode::NoData),
        };
    }

    let occupancy_percent = occupancy.occupancy_percent.unwrap_or(0.0).clamp(0.0, 100.0);
    let wait_time_minutes = match calibration {
        Some(calibration) => calibrated_wait_time(occupancy_percent, calibration),
        None => {
            DEFAULT_WAIT_TIME_MINUTES_AT_EMPTY
                + (occupancy_percent / 100.0)
                    * (DEFAULT_WAIT_TIME_MINUTES_AT_FULL - DEFAULT_WAIT_TIME_MINUTES_AT_EMPTY)
        }
    };
    let status = if matches!(occupancy.status, OccupancyStatus::Degraded) {
        WaitTimeStatus::Degraded
    } else {
        WaitTimeStatus::Ok
    };

    WaitTimeEstimate {
        wait_time_minutes: Some(wait_time_minutes),
        timestamp,
        status,
        error_code: None,
    }
}

fn calibrated_wait_time(occupancy_percent: f64, calibration: &CalibrationParams) -> f64 {
    let mut wait_time = calibration.intercept + calibration.slope * occupancy_percent;
    if let Some(min_wait_minutes) = calibration.min_wait_minutes {
        wait_time = wait_time.max(min_wait_minutes as f64);
    }
    if let Some(max_wait_minutes) = calibration.max_wait_minutes {
        wait_time = wait_time.min(max_wait_minutes as f64);
    }
    wait_time
}

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
    use crate::sensor::SensorRangeStatus;
    use crate::state::ReadingStatus;
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

    fn occupancy_reading(percent: Option<f64>, status: OccupancyStatus) -> OccupancyReading {
        OccupancyReading {
            occupancy_percent: percent,
            timestamp: UNIX_EPOCH,
            status,
        }
    }

    #[test]
    fn occupancy_mixed_valid_and_error_is_degraded() {
        let readings = vec![ok_reading(1, 800), ok_reading(2, 1500), error_reading(3)];

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

    #[test]
    fn wait_time_linear_conversion_uses_defaults() {
        let occupancy = occupancy_reading(Some(50.0), OccupancyStatus::Ok);

        let estimate = compute_wait_time(&occupancy, UNIX_EPOCH, None);

        assert_eq!(estimate.wait_time_minutes, Some(10.0));
        assert_eq!(estimate.status, WaitTimeStatus::Ok);
        assert_eq!(estimate.error_code, None);
    }

    #[test]
    fn wait_time_linear_conversion_hits_endpoints() {
        let empty = occupancy_reading(Some(0.0), OccupancyStatus::Ok);
        let full = occupancy_reading(Some(100.0), OccupancyStatus::Ok);

        let empty_estimate = compute_wait_time(&empty, UNIX_EPOCH, None);
        let full_estimate = compute_wait_time(&full, UNIX_EPOCH, None);

        assert_eq!(empty_estimate.wait_time_minutes, Some(0.0));
        assert_eq!(full_estimate.wait_time_minutes, Some(20.0));
    }

    #[test]
    fn wait_time_no_data_returns_degraded_no_data() {
        let occupancy = occupancy_reading(None, OccupancyStatus::NoData);

        let estimate = compute_wait_time(&occupancy, UNIX_EPOCH, None);

        assert_eq!(estimate.wait_time_minutes, None);
        assert_eq!(estimate.status, WaitTimeStatus::Degraded);
        assert_eq!(estimate.error_code, Some(WaitTimeErrorCode::NoData));
    }

    #[test]
    fn wait_time_degraded_without_no_data_keeps_degraded_status() {
        let occupancy = occupancy_reading(Some(75.0), OccupancyStatus::Degraded);

        let estimate = compute_wait_time(&occupancy, UNIX_EPOCH, None);

        assert_eq!(estimate.wait_time_minutes, Some(15.0));
        assert_eq!(estimate.status, WaitTimeStatus::Degraded);
        assert_eq!(estimate.error_code, None);
    }

    #[test]
    fn wait_time_clamps_out_of_range_inputs() {
        let high = occupancy_reading(Some(150.0), OccupancyStatus::Ok);
        let low = occupancy_reading(Some(-10.0), OccupancyStatus::Ok);

        let high_estimate = compute_wait_time(&high, UNIX_EPOCH, None);
        let low_estimate = compute_wait_time(&low, UNIX_EPOCH, None);

        assert_eq!(high_estimate.wait_time_minutes, Some(20.0));
        assert_eq!(low_estimate.wait_time_minutes, Some(0.0));
    }

    #[test]
    fn wait_time_uses_calibration_parameters() {
        let occupancy = occupancy_reading(Some(50.0), OccupancyStatus::Ok);
        let calibration = CalibrationParams {
            slope: 1.5,
            intercept: 2.0,
            min_wait_minutes: None,
            max_wait_minutes: None,
        };

        let estimate = compute_wait_time(&occupancy, UNIX_EPOCH, Some(&calibration));

        assert_eq!(estimate.wait_time_minutes, Some(77.0));
    }

    #[test]
    fn wait_time_calibration_clamps_to_bounds() {
        let occupancy = occupancy_reading(Some(100.0), OccupancyStatus::Ok);
        let calibration = CalibrationParams {
            slope: 1.0,
            intercept: 0.0,
            min_wait_minutes: Some(0),
            max_wait_minutes: Some(60),
        };

        let estimate = compute_wait_time(&occupancy, UNIX_EPOCH, Some(&calibration));

        assert_eq!(estimate.wait_time_minutes, Some(60.0));
    }
}
