use time::format_description::well_known::{self, iso8601::TimePrecision};
use tracing_subscriber::{fmt::time::UtcTime, EnvFilter};

pub fn setup_tracing() {
    let custom_iso = well_known::Iso8601::<
        {
            well_known::iso8601::Config::DEFAULT
                .set_time_precision(TimePrecision::Second { decimal_digits: None })
                .encode()
        },
    >;
    let timer = UtcTime::new(custom_iso);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_timer(timer)
        .init();
}
