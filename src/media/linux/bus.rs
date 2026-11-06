use super::mpris::{Interface, Properties, OBJECT_PATH};
use crate::media::MediaError;
use std::fmt::Display;
use std::io::ErrorKind;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;
use zbus::blocking::{connection, Connection};
use zbus::export::serde::Serialize;
use zbus::fdo;
use zbus::zvariant::{DynamicDeserialize, DynamicType, OwnedValue, Value};
use zbus::Message;

const CALL_TIMEOUT: Duration = Duration::from_millis(300);
const DBUS_NAME: &str = "org.freedesktop.DBus";
const DBUS_PATH: &str = "/org/freedesktop/DBus";
const PROPERTIES_INTERFACE: &str = "org.freedesktop.DBus.Properties";
const INTROSPECTABLE_INTERFACE: &str = "org.freedesktop.DBus.Introspectable";
const LIST_NAMES: &str = "ListNames";
const NAME_HAS_OWNER: &str = "NameHasOwner";
const GET_ALL: &str = "GetAll";
const SET: &str = "Set";
const INTROSPECT: &str = "Introspect";

#[derive(Default)]
pub(super) struct SessionBus {
    connection: Mutex<Option<Connection>>,
}

impl SessionBus {
    pub(super) fn names(&self) -> Result<(Connection, Vec<String>), MediaError> {
        self.with_connection(|connection| bus_call(connection, LIST_NAMES, &()))
    }

    pub(super) fn endpoint(&self, name: &str) -> Result<Endpoint, MediaError> {
        let (connection, owned): (_, bool) =
            self.with_connection(|connection| bus_call(connection, NAME_HAS_OWNER, &(name,)))?;
        if !owned {
            return Err(MediaError::SourceGone);
        }
        Ok(Endpoint::new(connection, name))
    }

    fn with_connection<T>(
        &self,
        call: impl Fn(&Connection) -> zbus::Result<T>,
    ) -> Result<(Connection, T), MediaError> {
        let connection = self.connection()?;
        if let Ok(value) = call(&connection) {
            return Ok((connection, value));
        }
        self.forget_connection();
        let connection = self.connection()?;
        let value = call(&connection).map_err(bus_failure)?;
        Ok((connection, value))
    }

    fn connection(&self) -> Result<Connection, MediaError> {
        let mut slot = self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(connection) = slot.as_ref() {
            return Ok(connection.clone());
        }
        let connection = connection::Builder::session()
            .and_then(|builder| builder.method_timeout(CALL_TIMEOUT).build())
            .map_err(|error| {
                MediaError::Backend(format!(
                    "Could not connect to the D-Bus session bus: {error}"
                ))
            })?;
        *slot = Some(connection.clone());
        Ok(connection)
    }

    fn forget_connection(&self) {
        *self
            .connection
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = None;
    }
}

pub(super) struct Endpoint {
    connection: Connection,
    pub(super) name: String,
}

impl Endpoint {
    pub(super) fn new(connection: Connection, name: &str) -> Self {
        Self {
            connection,
            name: name.to_owned(),
        }
    }

    pub(super) fn call<B>(
        &self,
        interface: Interface,
        method: &str,
        body: &B,
    ) -> Result<(), MediaError>
    where
        B: Serialize + DynamicType,
    {
        self.request(interface.name(), method, body).map(drop)
    }

    pub(super) fn get_all(&self, interface: Interface) -> Result<Properties, MediaError> {
        self.request(PROPERTIES_INTERFACE, GET_ALL, &(interface.name(),))?
            .body()
            .deserialize()
            .map_err(call_failure)
    }

    pub(super) fn get(&self, property: &str) -> Result<OwnedValue, MediaError> {
        self.get_all(Interface::Player)?
            .remove(property)
            .ok_or(MediaError::Unsupported)
    }

    pub(super) fn set(&self, property: &str, value: Value<'_>) -> Result<(), MediaError> {
        self.request(
            PROPERTIES_INTERFACE,
            SET,
            &(Interface::Player.name(), property, value),
        )
        .map(drop)
    }

    pub(super) fn introspect(&self) -> Result<String, MediaError> {
        self.request(INTROSPECTABLE_INTERFACE, INTROSPECT, &())?
            .body()
            .deserialize()
            .map_err(call_failure)
    }

    fn request<B>(&self, interface: &str, method: &str, body: &B) -> Result<Message, MediaError>
    where
        B: Serialize + DynamicType,
    {
        self.connection
            .call_method(
                Some(self.name.as_str()),
                OBJECT_PATH,
                Some(interface),
                method,
                body,
            )
            .map_err(call_failure)
    }
}

fn bus_call<B, R>(connection: &Connection, method: &str, body: &B) -> zbus::Result<R>
where
    B: Serialize + DynamicType,
    R: for<'d> DynamicDeserialize<'d>,
{
    connection
        .call_method(Some(DBUS_NAME), DBUS_PATH, Some(DBUS_NAME), method, body)?
        .body()
        .deserialize()
}

fn call_failure(error: zbus::Error) -> MediaError {
    match fdo::Error::from(error) {
        fdo::Error::ServiceUnknown(_) | fdo::Error::NameHasNoOwner(_) => MediaError::SourceGone,
        fdo::Error::ZBus(error) => bus_failure(error),
        error => backend_failure(error),
    }
}

fn bus_failure(error: zbus::Error) -> MediaError {
    match &error {
        zbus::Error::InputOutput(io) if io.kind() == ErrorKind::TimedOut => MediaError::TimedOut,
        _ => backend_failure(error),
    }
}

fn backend_failure(error: impl Display) -> MediaError {
    MediaError::Backend(format!("D-Bus error: {error}"))
}
