//! Interfaz D-Bus de sesión `dev.senes.Dot1` sobre `GDBus` (`GApplication`).
//!
//! Expone el objeto `/dev/senes/Dot1` en el bus de sesión del usuario
//! cuando la aplicación actúa como instancia principal.

use gnome_platform::adw::Application;
use gnome_platform::gio::DBusRegistration;
use std::rc::Rc;

pub const DBUS_INTERFACE: &str = "dev.senes.Dot1";
pub const DBUS_OBJECT_PATH: &str = "/dev/senes/Dot1";

const INTROSPECTION_XML: &str = r#"
<node>
  <interface name="dev.senes.Dot1">
    <method name="OpenUrl">
      <arg type="s" name="url" direction="in"/>
      <arg type="b" name="new_tab" direction="in"/>
    </method>
    <method name="NewTab">
      <arg type="s" name="url" direction="in"/>
    </method>
    <method name="NewWindow">
      <arg type="b" name="private" direction="in"/>
    </method>
    <method name="ListTabs">
      <arg type="a(tss)" name="tabs" direction="out"/>
    </method>
    <method name="ActivateTab">
      <arg type="t" name="tab_id" direction="in"/>
      <arg type="b" name="ok" direction="out"/>
    </method>
    <method name="CloseTab">
      <arg type="t" name="tab_id" direction="in"/>
      <arg type="b" name="ok" direction="out"/>
    </method>
    <method name="Reload"/>
    <method name="GetCurrentUrl">
      <arg type="s" name="url" direction="out"/>
    </method>
    <method name="GetVersion">
      <arg type="s" name="version" direction="out"/>
    </method>
  </interface>
</node>
"#;

pub type OpenUrlFn = Box<dyn Fn(&str, bool)>;

pub struct DbusHandlers {
    pub open_url: OpenUrlFn,
    pub new_window: Box<dyn Fn(bool)>,
    pub list_tabs: Box<dyn Fn() -> Vec<(u64, String, String)>>,
    pub activate_tab: Box<dyn Fn(u64) -> bool>,
    pub close_tab: Box<dyn Fn(u64) -> bool>,
    pub reload_active: Box<dyn Fn()>,
    pub current_url: Box<dyn Fn() -> String>,
}

pub struct DbusService {
    _registration: DBusRegistration,
}

impl DbusService {
    pub fn register(app: &Application, handlers: DbusHandlers) -> Option<Self> {
        let Some(conn) = app.dbus_connection() else {
            dot_core::debug!(
                "Sin conexión D-Bus en GApplication; se omite el registro de {DBUS_INTERFACE}"
            );
            return None;
        };

        let handlers = Rc::new(handlers);
        match conn.register_object(
            DBUS_OBJECT_PATH,
            INTROSPECTION_XML,
            DBUS_INTERFACE,
            move |method, params, invocation| match method {
                "OpenUrl" => {
                    let url = params.string(0).unwrap_or_default();
                    let new_tab = params.boolean(1).unwrap_or(true);
                    (handlers.open_url)(&url, new_tab);
                    invocation.return_unit();
                }
                "NewTab" => {
                    let url = params.string(0).unwrap_or_default();
                    (handlers.open_url)(&url, true);
                    invocation.return_unit();
                }
                "NewWindow" => {
                    let private = params.boolean(0).unwrap_or(false);
                    (handlers.new_window)(private);
                    invocation.return_unit();
                }
                "ListTabs" => {
                    let tabs = (handlers.list_tabs)();
                    invocation.return_tab_list(&tabs);
                }
                "ActivateTab" => {
                    let tab_id = params.uint64(0).unwrap_or(0);
                    let ok = (handlers.activate_tab)(tab_id);
                    invocation.return_bool(ok);
                }
                "CloseTab" => {
                    let tab_id = params.uint64(0).unwrap_or(0);
                    let ok = (handlers.close_tab)(tab_id);
                    invocation.return_bool(ok);
                }
                "Reload" => {
                    (handlers.reload_active)();
                    invocation.return_unit();
                }
                "GetCurrentUrl" => {
                    let url = (handlers.current_url)();
                    invocation.return_string(&url);
                }
                "GetVersion" => {
                    invocation.return_string(env!("CARGO_PKG_VERSION"));
                }
                other => {
                    invocation.return_error(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        &format!("Método D-Bus desconocido: {other}"),
                    );
                }
            },
        ) {
            Ok(registration) => {
                dot_core::info!(
                    "Interfaz D-Bus registrada: {DBUS_INTERFACE} en {DBUS_OBJECT_PATH}"
                );
                Some(Self {
                    _registration: registration,
                })
            }
            Err(err) => {
                dot_core::warn!("No se pudo registrar {DBUS_INTERFACE} en D-Bus: {err}");
                None
            }
        }
    }
}
