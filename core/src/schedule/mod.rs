use chrono::{DateTime, Datelike, NaiveDate, NaiveTime, Utc, Weekday};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct TimeInterval {
    pub start: NaiveTime,
    pub end: NaiveTime,
}

impl TimeInterval {
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 2 {
            return None;
        }
        let start = parse_time(parts[0])?;
        let end = parse_time(parts[1])?;
        Some(Self { start, end })
    }

    /// Semántica [start, end): cerrado a la izquierda, abierto a la derecha
    pub fn contains(&self, time: NaiveTime) -> bool {
        time >= self.start && time < self.end
    }
}

fn parse_time(s: &str) -> Option<NaiveTime> {
    let s = s.trim();
    if s.len() == 5 {
        NaiveTime::parse_from_str(s, "%H:%M").ok()
    } else if s.len() == 8 {
        NaiveTime::parse_from_str(s, "%H:%M:%S").ok()
    } else {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlertThreshold {
    Warning30,
    Warning10,
    Warning5,
    Countdown(u32),
    PeakActive,
}

#[derive(Debug, Clone)]
pub struct Schedule {
    pub id: String,
    pub intervals: Vec<TimeInterval>,
    pub weekdays: HashSet<Weekday>,
    pub exclude_chinese_public_holidays: bool,
    pub holidays: HashSet<NaiveDate>,
}

impl Schedule {
    pub fn new(
        id: &str,
        intervals_str: &[String],
        weekdays_str: &[String],
        exclude_chinese_public_holidays: bool,
        holidays_str: &[String],
    ) -> Self {
        let intervals = intervals_str
            .iter()
            .filter_map(|s| TimeInterval::parse(s))
            .collect();

        let weekdays = weekdays_str
            .iter()
            .filter_map(|w| match w.to_lowercase().as_str() {
                "mon" | "monday" | "lunes" => Some(Weekday::Mon),
                "tue" | "tuesday" | "martes" => Some(Weekday::Tue),
                "wed" | "wednesday" | "miercoles" => Some(Weekday::Wed),
                "thu" | "thursday" | "jueves" => Some(Weekday::Thu),
                "fri" | "friday" | "viernes" => Some(Weekday::Fri),
                "sat" | "saturday" | "sabado" => Some(Weekday::Sat),
                "sun" | "sunday" | "domingo" => Some(Weekday::Sun),
                _ => None,
            })
            .collect();

        let holidays = holidays_str
            .iter()
            .filter_map(|h| NaiveDate::parse_from_str(h, "%Y-%m-%d").ok())
            .collect();

        Self {
            id: id.to_string(),
            intervals,
            weekdays,
            exclude_chinese_public_holidays,
            holidays,
        }
    }

    /// Evalúa deterministamente si el timestamp UTC está en horario Peak
    pub fn is_peak(&self, time: DateTime<Utc>) -> bool {
        let weekday = time.weekday();
        // Fines de semana siempre off-peak
        if !self.weekdays.contains(&weekday) {
            return false;
        }

        // Si el perfil excluye festivos chinos y la fecha es festivo oficial
        if self.exclude_chinese_public_holidays {
            let date = time.date_naive();
            if self.holidays.contains(&date) {
                return false;
            }
        }

        // Evaluación de intervalos [inicio, fin)
        let current_time = time.time();
        self.intervals.iter().any(|inv| inv.contains(current_time))
    }

    /// Obtiene el inicio exacto de la ventana peak actual (si el timestamp está en peak)
    pub fn current_peak_window_start(&self, time: DateTime<Utc>) -> Option<DateTime<Utc>> {
        if !self.is_peak(time) {
            return None;
        }
        let date = time.date_naive();
        let current_time = time.time();
        for inv in &self.intervals {
            if inv.contains(current_time) {
                let start_dt = date.and_time(inv.start);
                return Some(DateTime::from_naive_utc_and_offset(start_dt, Utc));
            }
        }
        None
    }

    /// Obtiene el inicio del próximo intervalo peak y los segundos restantes
    pub fn next_peak(&self, time: DateTime<Utc>) -> Option<(DateTime<Utc>, u64)> {
        if self.is_peak(time) {
            return None;
        }

        // Proyectar hacia adelante hasta 8 días
        let mut check_dt = time;
        for _ in 0..(8 * 24 * 60) {
            check_dt += chrono::Duration::minutes(1);
            if self.is_peak(check_dt) {
                // Retroceder al segundo exacto de inicio
                let mut start_dt = check_dt;
                while self.is_peak(start_dt) {
                    start_dt -= chrono::Duration::seconds(1);
                }
                let exact_start = start_dt + chrono::Duration::seconds(1);
                let diff = (exact_start - time).num_seconds();
                if diff >= 0 {
                    return Some((exact_start, diff as u64));
                }
            }
        }
        None
    }

    /// Obtiene el inicio del próximo intervalo off-peak y los segundos restantes
    pub fn next_off_peak(&self, time: DateTime<Utc>) -> Option<(DateTime<Utc>, u64)> {
        if !self.is_peak(time) {
            return None;
        }

        let mut check_dt = time;
        for _ in 0..(24 * 60) {
            check_dt += chrono::Duration::minutes(1);
            if !self.is_peak(check_dt) {
                let mut start_dt = check_dt;
                while !self.is_peak(start_dt) {
                    start_dt -= chrono::Duration::seconds(1);
                }
                let exact_off = start_dt + chrono::Duration::seconds(1);
                let diff = (exact_off - time).num_seconds();
                if diff >= 0 {
                    return Some((exact_off, diff as u64));
                }
            }
        }
        None
    }

    /// Evalúa umbrales de alerta (30 min, 10 min, 5 min, cuenta regresiva 10s)
    pub fn evaluate_alert(&self, time: DateTime<Utc>) -> Option<AlertThreshold> {
        if self.is_peak(time) {
            return Some(AlertThreshold::PeakActive);
        }

        if let Some((_, seconds_remaining)) = self.next_peak(time) {
            if seconds_remaining <= 10 && seconds_remaining >= 1 {
                return Some(AlertThreshold::Countdown(seconds_remaining as u32));
            }
            if seconds_remaining == 300 {
                return Some(AlertThreshold::Warning5);
            }
            if seconds_remaining == 600 {
                return Some(AlertThreshold::Warning10);
            }
            if seconds_remaining == 1800 {
                return Some(AlertThreshold::Warning30);
            }
        }

        None
    }

    /// Retorna una instancia con el horario verificado oficial de DeepSeek Direct API
    pub fn default_deepseek() -> Self {
        Self::new(
            "deepseek_direct_verified",
            &["01:00-04:00".to_string(), "06:00-10:00".to_string()],
            &[
                "mon".to_string(),
                "tue".to_string(),
                "wed".to_string(),
                "thu".to_string(),
                "fri".to_string(),
            ],
            true,
            &chinese_public_holidays_2026(),
        )
    }
}

/// Lista oficial de festivos públicos de China verificados para el año 2026
pub fn chinese_public_holidays_2026() -> Vec<String> {
    vec![
        "2026-01-01".to_string(),
        "2026-02-15".to_string(),
        "2026-02-16".to_string(),
        "2026-02-17".to_string(),
        "2026-02-18".to_string(),
        "2026-02-19".to_string(),
        "2026-02-20".to_string(),
        "2026-02-21".to_string(),
        "2026-04-05".to_string(),
        "2026-05-01".to_string(),
        "2026-05-02".to_string(),
        "2026-05-03".to_string(),
        "2026-06-19".to_string(),
        "2026-09-25".to_string(),
        "2026-10-01".to_string(),
        "2026-10-02".to_string(),
        "2026-10-03".to_string(),
        "2026-10-04".to_string(),
        "2026-10-05".to_string(),
        "2026-10-06".to_string(),
        "2026-10-07".to_string(),
    ]
}
