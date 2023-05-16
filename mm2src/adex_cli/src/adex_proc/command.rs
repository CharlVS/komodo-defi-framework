use derive_more::Display;
use log::error;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub(crate) struct Command<T>
where
    T: Serialize + Sized,
{
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub flatten_data: Option<T>,
    pub userpass: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<Method>,
}

#[derive(Serialize, Clone, Display)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Method {
    Stop,
    Version,
    #[serde(rename = "my_balance")]
    GetBalance,
    #[serde(rename = "get_enabled_coins")]
    GetEnabledCoins,
    #[serde(rename = "orderbook")]
    GetOrderbook,
    Sell,
    Buy,
}

#[derive(Serialize, Clone, Copy, Display)]
pub(crate) struct Dummy {}

impl<T> Command<T>
where
    T: Serialize + Sized,
{
    pub fn builder() -> CommandBuilder<T> { CommandBuilder::new() }
}

pub(crate) struct CommandBuilder<T> {
    userpass: Option<String>,
    method: Option<Method>,
    flatten_data: Option<T>,
}

impl<T> CommandBuilder<T>
where
    T: Serialize,
{
    fn new() -> Self {
        CommandBuilder {
            userpass: None,
            method: None,
            flatten_data: None,
        }
    }

    pub(crate) fn userpass(&mut self, userpass: String) -> &mut Self {
        self.userpass = Some(userpass);
        self
    }

    pub(crate) fn method(&mut self, method: Method) -> &mut Self {
        self.method = Some(method);
        self
    }

    pub(crate) fn flatten_data(&mut self, flatten_data: T) -> &mut Self {
        self.flatten_data = Some(flatten_data);
        self
    }

    pub(crate) fn build(&mut self) -> Command<T> {
        Command {
            userpass: self
                .userpass
                .take()
                .ok_or_else(|| error!("Build command failed, no userpass"))
                .expect("Unexpected error during building api command"),
            method: self.method.take(),
            flatten_data: self.flatten_data.take(),
        }
    }
}

impl<T: Serialize + Clone> std::fmt::Display for Command<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut cmd: Self = self.clone();
        cmd.userpass = "***********".to_string();
        writeln!(
            f,
            "{}",
            serde_json::to_string(&cmd).unwrap_or_else(|_| "Unknown".to_string())
        )
    }
}