# Nodo edge Raspberry Pi (futuro)

Este directorio contendrá el nodo edge que correrá en Raspberry Pi como
nodo ligero de la red WOMBAT.

## Alcance MVP

- No implementado todavía.
- Futuro: nodo libp2p ligero, lecturas GPIO de sensores locales, publicación
  de WONBs de tipo `context`.
- No incluir claves privadas ni secretos en el repositorio.

## Cómo correr (futuro)

```bash
# Compilar para ARM (Raspberry Pi) desde la estación de trabajo:
cargo build --release --target armv7-unknown-linux-gnueabihf
```

## Responsable sugerido

Jelibeth.
