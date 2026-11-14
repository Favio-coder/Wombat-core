"""Guarda tu login de Earthdata en ~/.netrc para poder leer granules NISAR.

Lo corres TU, en tu propia terminal: te pide usuario y contrasena de
https://urs.earthdata.nasa.gov (cuenta gratuita) y no los muestra ni los sube a ningun lado.

Uso:  python data-pipeline/validacion/earthdata_login.py
"""
try:  # usa los certificados de Windows si la red inspecciona HTTPS
    import truststore
    truststore.inject_into_ssl()
except ImportError:
    pass

import earthaccess

if __name__ == "__main__":
    auth = earthaccess.login(strategy="interactive", persist=True)
    print("Login guardado en ~/.netrc." if auth.authenticated else "No se pudo iniciar sesion.")
