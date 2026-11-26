// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Alfonso García Tejeo <agtejeo@ugr.es>
//
// Este fichero forma parte de RetroTerm. RetroTerm es software libre: puedes
// redistribuirlo y/o modificarlo bajo los términos de la Licencia Pública
// General de GNU (GPL) publicada por la Free Software Foundation, versión 3
// o (a tu elección) cualquier versión posterior. Se distribuye SIN NINGUNA
// GARANTÍA. Ver el fichero LICENSE.

//! Conexión DIRECTA con la librería de C `libpam.so` (FFI propio).
//!
//! FFI ("Foreign Function Interface") = llamar desde Rust a código escrito en
//! otro lenguaje, aquí C. Antes usábamos el crate `pam`, que generaba esta
//! descripción al compilar con `bindgen` (y para eso necesitaba libclang).
//! Como solo usamos cuatro funciones, la escribimos a mano: es lo mismo que
//! dice la cabecera `/usr/include/security/pam_appl.h`, pero en Rust.
//!
//! ─── `unsafe` ───────────────────────────────────────────────────────────────
//! El compilador de Rust no puede revisar lo que hace el código de C: no sabe
//! si un puntero es válido ni quién libera cada trozo de memoria. Por eso
//! toda llamada a C va dentro de `unsafe { }`, que significa "esto lo
//! garantizo YO". Regla de este módulo: todo el `unsafe` se queda AQUÍ
//! dentro, y hacia fuera solo se ofrece una función segura, `autenticar`.
//!
//! ─── Cómo pregunta PAM ──────────────────────────────────────────────────────
//! A PAM no se le pasa la contraseña: PAM la PIDE. Al empezar le damos una
//! función nuestra (la "conversación"); cuando un módulo de PAM necesita algo
//! ("Password:"), llama a esa función y nosotros contestamos. Es decir: C
//! llama a una función escrita en Rust (un "callback").

use std::ffi::{CStr, CString};
use std::ptr;

// Los tipos de C, con su tamaño exacto en esta máquina: `c_int` es el `int`
// de C (32 bits en Linux), `c_char` su `char`, `c_void` el "algo" de `void*`.
use libc::{c_char, c_int, c_void};

// ---- Constantes de pam_appl.h / _pam_types.h
//
// Linux usa Linux-PAM y macOS usa OpenPAM: las funciones y las estructuras
// son las mismas, pero algunos códigos de error tienen OTRO número.
// Estos coinciden en los dos:
const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
// Y este no. `#[cfg(...)]` es "compilación condicional": la línea que le
// sigue solo existe si se cumple la condición. Así hay UNA sola constante
// `PAM_CONV_ERR` en cada sistema, con su valor correcto.
// `not(target_os = "macos")`: en todo lo que no sea macOS (Linux…).
#[cfg(not(target_os = "macos"))]
const PAM_CONV_ERR: c_int = 19; // Linux-PAM (_pam_types.h)
#[cfg(target_os = "macos")]
const PAM_CONV_ERR: c_int = 6; // OpenPAM (security/pam_constants.h)
/// Tipos de mensaje: pedir algo SIN eco (contraseña) o CON eco (usuario).
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;

// ---- Estructuras de C
//
// `#[repr(C)]`: "coloca los campos en memoria exactamente como lo haría C",
// en este orden y con su relleno. Sin esto Rust podría reordenarlos, y C
// leería basura.

/// Un mensaje de PAM (`struct pam_message`).
#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

/// Nuestra respuesta a un mensaje (`struct pam_response`).
#[repr(C)]
struct PamResponse {
    /// El texto que contestamos. PAM lo LIBERA con `free()` de C, así que
    /// hay que reservarlo con `malloc` de C, nunca con un String de Rust.
    resp: *mut c_char,
    resp_retcode: c_int, // sin uso; siempre 0
}

/// La conversación (`struct pam_conv`): la función y un puntero "de usuario"
/// que PAM nos devuelve tal cual en cada llamada (por ahí nos llegan el
/// usuario y la contraseña).
#[repr(C)]
struct PamConv {
    /// `Option<extern "C" fn ...>` es como un puntero a función de C que
    /// puede ser NULL (None). `extern "C"`: se llama "a la manera de C".
    conv: Option<
        extern "C" fn(
            num_msg: c_int,
            msg: *mut *const PamMessage,
            resp: *mut *mut PamResponse,
            appdata_ptr: *mut c_void,
        ) -> c_int,
    >,
    appdata_ptr: *mut c_void,
}

/// El "asa" de una transacción PAM. Para nosotros es una caja negra: solo
/// pasamos su puntero de una función a otra. Un struct VACÍO y privado es
/// la forma habitual de representar en Rust un tipo opaco de C.
#[repr(C)]
struct PamHandle {
    _privado: [u8; 0],
}

// ---- Las funciones de libpam.so
//
// `#[link(name = "pam")]`: al enlazar, usar libpam.so (por eso sigue
// haciendo falta pam-devel / libpam0g-dev, que trae ese enlace).
// `extern "C" { ... }`: aquí solo DECLARAMOS; el código está en la librería.
#[link(name = "pam")]
extern "C" {
    fn pam_start(
        service_name: *const c_char,
        user: *const c_char,
        pam_conversation: *const PamConv,
        pamh: *mut *mut PamHandle,
    ) -> c_int;
    fn pam_authenticate(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_acct_mgmt(pamh: *mut PamHandle, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut PamHandle, pam_status: c_int) -> c_int;
    fn pam_strerror(pamh: *mut PamHandle, errnum: c_int) -> *const c_char;
}

/// Lo que la conversación necesita para contestar. Los textos llevan el `\0`
/// final que espera C.
struct Credenciales {
    usuario: CString,
    /// La contraseña como bytes (con su `\0`) y no como CString: así, al
    /// terminar, podemos sobrescribirla con ceros (ver `Drop`).
    clave: Vec<u8>,
}

/// `Drop` es el "destructor": se ejecuta solo cuando el valor deja de
/// existir, salga como salga la función (también si hay un error y `?`).
impl Drop for Credenciales {
    fn drop(&mut self) {
        // Borrar la contraseña de la memoria antes de devolverla al sistema.
        // `write_volatile`: obliga a escribir de verdad (un `= 0` normal el
        // compilador podría quitarlo, al ver que nadie lee ya esos bytes).
        for byte in self.clave.iter_mut() {
            // SAFETY: `byte` es una referencia válida a un byte del Vec.
            unsafe { ptr::write_volatile(byte, 0) };
        }
    }
}

/// La transacción PAM abierta. Con `Drop`, `pam_end` se llama SIEMPRE.
struct Transaccion {
    asa: *mut PamHandle,
    /// Resultado de la última operación: `pam_end` quiere saberlo.
    estado: c_int,
}

impl Drop for Transaccion {
    fn drop(&mut self) {
        // SAFETY: `asa` lo dio `pam_start` y no se ha cerrado antes.
        unsafe { pam_end(self.asa, self.estado) };
    }
}

impl Transaccion {
    /// El código de PAM traducido a texto ("Authentication failure"…).
    fn texto_error(&self, codigo: c_int) -> String {
        // SAFETY: pam_strerror devuelve un texto fijo de la librería (o NULL).
        let texto = unsafe { pam_strerror(self.asa, codigo) };
        if texto.is_null() {
            format!("error {codigo}")
        } else {
            // `CStr::from_ptr`: "mira" un texto de C (sin copiarlo) hasta su `\0`.
            let texto = unsafe { CStr::from_ptr(texto) };
            format!("{} ({codigo})", texto.to_string_lossy())
        }
    }
}

/// La conversación: PAM la llama con `num_msg` mensajes y espera una lista
/// de otras tantas respuestas, reservada con `malloc` (la liberará PAM).
extern "C" fn conversar(
    num_msg: c_int,
    msg: *mut *const PamMessage,
    resp: *mut *mut PamResponse,
    appdata_ptr: *mut c_void,
) -> c_int {
    if num_msg <= 0 || msg.is_null() || resp.is_null() || appdata_ptr.is_null() {
        return PAM_CONV_ERR;
    }
    let num = num_msg as usize;
    // SAFETY: `appdata_ptr` es el puntero a Credenciales que pasamos en
    // `autenticar`, que sigue vivo mientras dura la transacción.
    let credenciales = unsafe { &*(appdata_ptr as *const Credenciales) };

    // `calloc` (de C): reserva `num` respuestas y las deja a cero (NULL).
    // SAFETY: llamada normal a la librería de C.
    let respuestas =
        unsafe { libc::calloc(num, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if respuestas.is_null() {
        return PAM_BUF_ERR;
    }

    for i in 0..num {
        // SAFETY: PAM garantiza `num` mensajes válidos. (Linux-PAM pasa un
        // array de PUNTEROS a mensaje: `msg[i]`.)
        let mensaje = unsafe { &**msg.add(i) };
        let contestacion: *const c_char = match mensaje.msg_style {
            PAM_PROMPT_ECHO_OFF => credenciales.clave.as_ptr() as *const c_char,
            PAM_PROMPT_ECHO_ON => credenciales.usuario.as_ptr(),
            // Avisos o errores informativos: no hay nada que contestar.
            _ => continue,
        };
        // `strdup` (de C): copia el texto en memoria de `malloc`.
        // SAFETY: `contestacion` apunta a un texto acabado en `\0`.
        let copia = unsafe { libc::strdup(contestacion) };
        if copia.is_null() {
            liberar(respuestas, num);
            return PAM_BUF_ERR;
        }
        // SAFETY: `i < num`, dentro de lo reservado.
        unsafe { (*respuestas.add(i)).resp = copia };
    }

    // SAFETY: `resp` es válido (lo comprobamos arriba): ahí dejamos la lista.
    unsafe { *resp = respuestas };
    PAM_SUCCESS
}

/// Si algo falla a medias en `conversar`, libera lo ya reservado (borrando
/// antes los textos, que pueden ser la contraseña).
fn liberar(respuestas: *mut PamResponse, num: usize) {
    for i in 0..num {
        // SAFETY: `respuestas` tiene `num` elementos (de calloc).
        unsafe {
            let texto = (*respuestas.add(i)).resp;
            if !texto.is_null() {
                libc::memset(texto as *mut c_void, 0, libc::strlen(texto));
                libc::free(texto as *mut c_void);
            }
        }
    }
    // SAFETY: memoria de calloc, liberada una sola vez.
    unsafe { libc::free(respuestas as *mut c_void) };
}

/// ¿Acepta el servicio PAM `servicio` la contraseña `clave` de `usuario`?
///
/// Hace lo mismo que hacía el crate `pam`: `pam_authenticate` (¿es la
/// contraseña?) y `pam_acct_mgmt` (¿la cuenta está activa, no caducada…?).
/// El error trae el texto de PAM, para la consola.
pub fn autenticar(servicio: &str, usuario: &str, clave: &str) -> Result<(), String> {
    // `CString::new` falla si el texto tiene un `\0` dentro (C lo tomaría
    // por el final). `map_err` cambia ese error por nuestro mensaje.
    let servicio = CString::new(servicio).map_err(|_| "nombre de servicio no válido")?;
    let credenciales = Credenciales {
        usuario: CString::new(usuario).map_err(|_| "nombre de usuario no válido")?,
        clave: CString::new(clave)
            .map_err(|_| "la contraseña contiene un carácter nulo")?
            .into_bytes_with_nul(),
    };
    let conversacion = PamConv {
        conv: Some(conversar),
        // `&credenciales as *const _`: un puntero crudo a nuestras
        // credenciales. Sigue siendo válido porque `credenciales` vive hasta
        // el final de esta función, después de cerrar la transacción.
        appdata_ptr: &credenciales as *const Credenciales as *mut c_void,
    };

    let mut asa: *mut PamHandle = ptr::null_mut();
    // SAFETY: todos los punteros son válidos durante la llamada; `asa` lo
    // rellena PAM.
    let codigo = unsafe {
        pam_start(
            servicio.as_ptr(),
            credenciales.usuario.as_ptr(),
            &conversacion,
            &mut asa,
        )
    };
    if codigo != PAM_SUCCESS || asa.is_null() {
        return Err(format!("no se pudo iniciar (error {codigo})"));
    }
    // Desde aquí, pase lo que pase, `Transaccion::drop` llamará a pam_end.
    let mut transaccion = Transaccion {
        asa,
        estado: PAM_SUCCESS,
    };

    // SAFETY: `asa` es válido hasta el `drop` de `transaccion`.
    transaccion.estado = unsafe { pam_authenticate(asa, 0) };
    if transaccion.estado != PAM_SUCCESS {
        return Err(transaccion.texto_error(transaccion.estado));
    }
    transaccion.estado = unsafe { pam_acct_mgmt(asa, 0) };
    if transaccion.estado != PAM_SUCCESS {
        return Err(transaccion.texto_error(transaccion.estado));
    }
    Ok(())
    // Al salir, Rust destruye en orden INVERSO a la creación: primero
    // `transaccion` (pam_end), luego `conversacion` y por último
    // `credenciales` (que borra la contraseña). Justo el orden correcto.
}
