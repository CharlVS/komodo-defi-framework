#[macro_export]
macro_rules! writeln_field {
    ($writer:expr, $name:expr, $value:expr, $width:expr) => {
        writeln_safe_io!($writer, "{:>width$}: {}", $name, $value, width = $width);
    };
}

#[macro_export]
macro_rules! write_field_option {
    ($writer:expr, $name:expr, $value:expr, $width:expr) => {
        if let Some(ref value) = $value {
            writeln_safe_io!($writer, "{:>width$}: {}", $name, value, width = $width);
        }
    };
}

#[macro_export]
macro_rules! write_confirmation_settings {
    ($writer:expr, $host:ident, $width:ident) => {
        if $host.conf_settings.is_some() {
            let output = format_confirmation_settings($host.conf_settings.as_ref().unwrap());
            writeln_field!($writer, concat!(stringify!($host), ".conf_settings"), output, $width);
        }
    };
}

#[macro_export]
macro_rules! write_base_rel {
    ($writer:ident, $host:expr, $width:ident) => {
        writeln_field!(
            $writer,
            concat!(stringify!($host), ".(base,rel)"),
            format!(
                "{}({}), {}({})",
                $host.base, $host.base_amount, $host.rel, $host.rel_amount
            ),
            $width
        );
    };
}

#[macro_export]
macro_rules! write_connected {
    ($writer:ident, $connected:expr, $width:ident) => {
        writeln_field!(
            $writer,
            concat!(stringify!($connected), ".(taker,maker)"),
            format!("{},{}", $connected.taker_order_uuid, $connected.maker_order_uuid),
            $width
        );
        writeln_field!(
            $writer,
            concat!(stringify!($connected), ".(sender, dest)"),
            format!("{},{}", $connected.sender_pubkey, $connected.dest_pub_key),
            $width
        );
    };
}

pub(crate) use {write_base_rel, write_confirmation_settings, write_connected, write_field_option, writeln_field};
