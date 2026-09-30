//! Cron expression parser

use chrono::{DateTime, Utc, Datelike, Timelike};
use serde::{Deserialize, Serialize};

/// Parsed cron expression
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronExpression {
    pub minutes: Vec<u32>,
    pub hours: Vec<u32>,
    pub days_of_month: Vec<u32>,
    pub months: Vec<u32>,
    pub days_of_week: Vec<u32>,
}

/// Cron parser for scheduling
pub struct CronParser;

impl CronParser {
    /// Parse a cron expression (5-field format: minute hour day month weekday)
    pub fn parse(expr: &str) -> Result<CronExpression, String> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        
        if fields.len() != 5 {
            return Err("Cron expression must have 5 fields".to_string());
        }

        Ok(CronExpression {
            minutes: Self::parse_field(fields[0], 0, 59)?,
            hours: Self::parse_field(fields[1], 0, 23)?,
            days_of_month: Self::parse_field(fields[2], 1, 31)?,
            months: Self::parse_field(fields[3], 1, 12)?,
            days_of_week: Self::parse_field(fields[4], 0, 6)?,
        })
    }

    /// Parse a single cron field
    fn parse_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
        let mut values = Vec::new();

        for part in field.split(',') {
            if part == "*" {
                values.extend(min..=max);
            } else if part.contains('/') {
                // Step values: */5 means every 5
                let parts: Vec<&str> = part.split('/').collect();
                if parts.len() != 2 {
                    return Err(format!("Invalid step expression: {}", part));
                }
                
                let start = if parts[0] == "*" { min } else {
                    parts[0].parse::<u32>().map_err(|_| format!("Invalid value: {}", parts[0]))?
                };
                let step = parts[1].parse::<u32>().map_err(|_| format!("Invalid step: {}", parts[1]))?;
                
                if step == 0 {
                    return Err("Step cannot be zero".to_string());
                }
                
                let mut i = start;
                while i <= max {
                    values.push(i);
                    i += step;
                }
            } else if part.contains('-') {
                // Range: 1-5 means 1,2,3,4,5
                let range: Vec<&str> = part.split('-').collect();
                if range.len() != 2 {
                    return Err(format!("Invalid range: {}", part));
                }
                
                let start = range[0].parse::<u32>().map_err(|_| format!("Invalid start: {}", range[0]))?;
                let end = range[1].parse::<u32>().map_err(|_| format!("Invalid end: {}", range[1]))?;
                
                if start > end {
                    return Err(format!("Range start {} > end {}", start, end));
                }
                
                values.extend(start..=end);
            } else {
                // Single value
                let value = part.parse::<u32>().map_err(|_| format!("Invalid value: {}", part))?;
                if value < min || value > max {
                    return Err(format!("Value {} out of range {}-{}", value, min, max));
                }
                values.push(value);
            }
        }

        values.sort();
        values.dedup();
        Ok(values)
    }

    /// Check if a cron expression matches a datetime
    pub fn matches(expr: &CronExpression, dt: &DateTime<Utc>) -> bool {
        expr.minutes.contains(&dt.minute())
            && expr.hours.contains(&dt.hour())
            && expr.days_of_month.contains(&dt.day())
            && expr.months.contains(&dt.month())
            && expr.days_of_week.contains(&dt.weekday().num_days_from_sunday())
    }

    /// Get the next execution time
    pub fn next_execution(expr: &CronExpression, after: &DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut candidate = *after + chrono::Duration::minutes(1);
        candidate = candidate.with_second(0).unwrap_or(candidate);
        
        // Search for next match (up to 1 year ahead)
        for _ in 0..525600 { // 365 * 24 * 60
            if Self::matches(expr, &candidate) {
                return Some(candidate);
            }
            candidate += chrono::Duration::minutes(1);
        }
        
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_wildcard() {
        let expr = CronParser::parse("* * * * *").unwrap();
        assert_eq!(expr.minutes.len(), 60);
        assert_eq!(expr.hours.len(), 24);
    }

    #[test]
    fn test_parse_specific() {
        let expr = CronParser::parse("0 12 * * *").unwrap();
        assert_eq!(expr.minutes, vec![0]);
        assert_eq!(expr.hours, vec![12]);
    }

    #[test]
    fn test_parse_step() {
        let expr = CronParser::parse("*/15 * * * *").unwrap();
        assert_eq!(expr.minutes, vec![0, 15, 30, 45]);
    }

    #[test]
    fn test_parse_range() {
        let expr = CronParser::parse("0 9-17 * * *").unwrap();
        assert_eq!(expr.hours, (9..=17).collect::<Vec<u32>>());
    }
}
