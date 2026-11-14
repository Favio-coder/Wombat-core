//! Aritmética exacta para la ruta que produce el payload (R16).
//!
//! Nada de `f64` aquí dentro. El score se calcula con racionales de precisión
//! arbitraria acotada (`i128`) y solo al final se convierte a puntos básicos.
//! La razón no es purismo: `0.45 * 0.12 + 0.35 * 0.05 + 0.20 * (3/31)` en
//! binario depende del orden de las operaciones y de la plataforma, y un bp de
//! diferencia cambia el rank de una celda y, con él, qué va a verificar una
//! persona en el campo.
//!
//! Los números de entrada (`cells.geojson`, `risk-config.json`) son decimales
//! JSON. [`Exact::from_f64_repr`] los convierte **a través de su
//! representación decimal más corta**, no de sus bits: `0.95` entra como 95/100
//! y no como 0.9499999999999999555910790149937383830547332763671875.

use std::cmp::Ordering;

use crate::error::RiskError;

/// Un racional exacto `num/den`, siempre normalizado y con `den > 0`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exact {
    num: i128,
    den: i128,
}

impl Exact {
    /// Cero.
    pub const ZERO: Exact = Exact { num: 0, den: 1 };
    /// Uno.
    pub const ONE: Exact = Exact { num: 1, den: 1 };

    /// Construye `num/den` normalizado.
    pub fn new(num: i128, den: i128) -> Result<Self, RiskError> {
        if den == 0 {
            return Err(RiskError::Arithmetic("denominador cero".into()));
        }
        let sign = if den < 0 { -1 } else { 1 };
        let g = gcd(num.unsigned_abs(), den.unsigned_abs()).max(1) as i128;
        Ok(Exact {
            num: sign * num / g,
            den: sign * den / g,
        })
    }

    /// Un entero.
    pub fn int(n: i128) -> Self {
        Exact { num: n, den: 1 }
    }

    /// Convierte un número JSON a su decimal exacto.
    ///
    /// Pasa por la representación decimal más corta que vuelve al mismo `f64`
    /// (la que imprime `Display` de Rust, que nunca usa notación científica).
    /// Así `0.95` es 95/100: el valor que una persona escribió en el fixture, no
    /// el binario más cercano.
    pub fn from_f64_repr(x: f64) -> Result<Self, RiskError> {
        if !x.is_finite() {
            return Err(RiskError::Arithmetic(format!("número no finito: {x}")));
        }
        Self::from_decimal_str(&format!("{x}"))
    }

    /// Convierte un decimal escrito (`-12.345`) a racional exacto.
    pub fn from_decimal_str(s: &str) -> Result<Self, RiskError> {
        let bad = || RiskError::Arithmetic(format!("decimal inválido: {s}"));
        let (sign, digits) = match s.strip_prefix('-') {
            Some(rest) => (-1i128, rest),
            None => (1i128, s),
        };
        let (int_part, frac_part) = match digits.split_once('.') {
            Some((a, b)) => (a, b),
            None => (digits, ""),
        };
        if int_part.is_empty() && frac_part.is_empty() {
            return Err(bad());
        }
        if !int_part.bytes().all(|b| b.is_ascii_digit())
            || !frac_part.bytes().all(|b| b.is_ascii_digit())
        {
            return Err(bad());
        }
        let mut num: i128 = 0;
        for b in int_part.bytes().chain(frac_part.bytes()) {
            num = num
                .checked_mul(10)
                .and_then(|n| n.checked_add(i128::from(b - b'0')))
                .ok_or_else(bad)?;
        }
        let den = 10i128.checked_pow(frac_part.len() as u32).ok_or_else(bad)?;
        Exact::new(sign * num, den)
    }

    /// Suma exacta.
    pub fn add(self, other: Exact) -> Result<Self, RiskError> {
        let num = mul(self.num, other.den)?.checked_add(mul(other.num, self.den)?);
        Exact::new(num.ok_or_else(overflow)?, mul(self.den, other.den)?)
    }

    /// Producto exacto.
    pub fn mul(self, other: Exact) -> Result<Self, RiskError> {
        Exact::new(mul(self.num, other.num)?, mul(self.den, other.den)?)
    }

    /// División exacta. Dividir por cero es un error, no un infinito.
    pub fn div(self, other: Exact) -> Result<Self, RiskError> {
        if other.num == 0 {
            return Err(RiskError::Arithmetic("división por cero".into()));
        }
        Exact::new(mul(self.num, other.den)?, mul(self.den, other.num)?)
    }

    /// El menor de los dos.
    pub fn min(self, other: Exact) -> Self {
        if self.cmp_exact(&other) == Ordering::Greater {
            other
        } else {
            self
        }
    }

    /// Comparación exacta, sin pasar por coma flotante.
    pub fn cmp_exact(&self, other: &Exact) -> Ordering {
        // den > 0 siempre, así que el signo no se invierte al multiplicar.
        mul(self.num, other.den)
            .unwrap_or(i128::MAX)
            .cmp(&mul(other.num, self.den).unwrap_or(i128::MAX))
    }

    /// Puntos básicos, con redondeo half-up: `floor(x * 10000 + 1/2)`.
    ///
    /// Es la regla que fijó `gen_fixture_bundle.py` y que documenta
    /// `contracts/v1/fixtures/expected/ranking.json`.
    pub fn to_bp(self) -> Result<i128, RiskError> {
        // floor((20000*num + den) / (2*den)), exacto y sin flotantes.
        let numerator = mul(20_000, self.num)?
            .checked_add(self.den)
            .ok_or_else(overflow)?;
        let denominator = mul(2, self.den)?;
        Ok(numerator.div_euclid(denominator))
    }

    /// Puntos básicos como `u32`, rechazando valores fuera de `0..=10000`.
    pub fn to_bp_u32(self) -> Result<u32, RiskError> {
        let bp = self.to_bp()?;
        u32::try_from(bp)
            .ok()
            .filter(|v| *v <= 10_000)
            .ok_or(RiskError::BpOutOfRange { bp })
    }

    /// Redondeo a `decimals` cifras decimales, half-even (el `.1f` de Python).
    ///
    /// DECISIÓN: half-even, no half-up, porque es lo que hace el `format` de
    /// Python que generó los textos del fixture. Para un valor decimal
    /// exactamente a mitad de camino (p. ej. 0.15 km) Python redondea sobre el
    /// binario más cercano y puede bajar donde esta función sube; en el fixture
    /// no ocurre, porque todos los metros son múltiplos de 100. Rust pasa a ser
    /// el productor del bundle (R15), así que esta es la regla desde ahora.
    pub fn round_half_even_str(self, decimals: u32) -> Result<String, RiskError> {
        let scale = 10i128.checked_pow(decimals).ok_or_else(overflow)?;
        let scaled = mul(self.num, scale)?;
        let (mut q, r) = (scaled.div_euclid(self.den), scaled.rem_euclid(self.den));
        let twice = mul(2, r)?;
        match twice.cmp(&self.den) {
            Ordering::Greater => q += 1,
            Ordering::Equal if q % 2 != 0 => q += 1,
            _ => {}
        }
        Ok(render_fixed(q, scale, decimals))
    }

    /// Texto del valor sin ceros finales, como el `%g` de Python para los
    /// números del contrato (`3.0` → `"3"`, `3.5` → `"3.5"`).
    ///
    /// DECISIÓN: `%g` cambia a notación exponencial por debajo de 1e-4 y a
    /// partir de 1e6; aquí no, porque los valores que lo usan son umbrales en
    /// dB acotados por el esquema. Si algún día el contrato admite esos
    /// órdenes de magnitud, esto hay que revisarlo.
    pub fn trimmed_decimal(self) -> Result<String, RiskError> {
        if self.den == 1 {
            return Ok(self.num.to_string());
        }
        // Un decimal finito tiene denominador 2^a·5^b: hacen falta max(a,b)
        // cifras. Cualquier otro factor (1/3, 3/31) no es decimal finito y
        // mentir con una aproximación aquí sería peor que fallar.
        let (mut den, mut twos, mut fives) = (self.den, 0u32, 0u32);
        while den % 2 == 0 {
            den /= 2;
            twos += 1;
        }
        while den % 5 == 0 {
            den /= 5;
            fives += 1;
        }
        if den != 1 {
            return Err(RiskError::Arithmetic(format!(
                "{}/{} no tiene representación decimal finita",
                self.num, self.den
            )));
        }
        let decimals = twos.max(fives);
        let scale = 10i128.checked_pow(decimals).ok_or_else(overflow)?;
        let scaled = mul(self.num, scale / self.den)?;
        let text = render_fixed(scaled, scale, decimals);
        let text = text.trim_end_matches('0');
        Ok(text.trim_end_matches('.').to_string())
    }
}

/// Escribe `q / scale` con exactamente `decimals` cifras decimales.
fn render_fixed(q: i128, scale: i128, decimals: u32) -> String {
    if decimals == 0 {
        return q.to_string();
    }
    let sign = if q < 0 { "-" } else { "" };
    let q = q.unsigned_abs();
    let scale = scale.unsigned_abs();
    format!(
        "{sign}{}.{:0width$}",
        q / scale,
        q % scale,
        width = decimals as usize
    )
}

fn mul(a: i128, b: i128) -> Result<i128, RiskError> {
    a.checked_mul(b).ok_or_else(overflow)
}

fn overflow() -> RiskError {
    RiskError::Arithmetic("desborde en aritmética exacta".into())
}

fn gcd(mut a: u128, mut b: u128) -> u128 {
    while b != 0 {
        let t = a % b;
        a = b;
        b = t;
    }
    a
}

/// Porcentaje entero a partir de puntos básicos, con el redondeo del fixture:
/// `(bp + 50) / 100`.
pub fn bp_to_percent(bp: u32) -> u32 {
    (bp + 50) / 100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decimal_strings_are_exact() {
        assert_eq!(
            Exact::from_decimal_str("0.95").unwrap(),
            Exact::new(19, 20).unwrap()
        );
        assert_eq!(Exact::from_decimal_str("3").unwrap(), Exact::int(3));
        assert_eq!(
            Exact::from_decimal_str("-0.25").unwrap(),
            Exact::new(-1, 4).unwrap()
        );
    }

    #[test]
    fn f64_goes_through_its_shortest_decimal() {
        // 0.95 en binario no es 19/20, pero su repr más corta sí.
        assert_eq!(
            Exact::from_f64_repr(0.95).unwrap(),
            Exact::new(19, 20).unwrap()
        );
        assert_eq!(
            Exact::from_f64_repr(0.1).unwrap(),
            Exact::new(1, 10).unwrap()
        );
    }

    #[test]
    fn bp_rounds_half_up() {
        assert_eq!(
            Exact::from_decimal_str("0.95").unwrap().to_bp().unwrap(),
            9500
        );
        assert_eq!(Exact::new(3, 31).unwrap().to_bp().unwrap(), 968); // 967.74…
                                                                      // Exactamente a mitad: 0.00005 -> 0.5 bp -> sube a 1.
        assert_eq!(Exact::new(1, 20_000).unwrap().to_bp().unwrap(), 1);
    }

    #[test]
    fn bp_out_of_range_rejected() {
        let err = Exact::int(2).to_bp_u32().unwrap_err();
        assert!(matches!(err, RiskError::BpOutOfRange { .. }), "{err}");
    }

    #[test]
    fn division_by_zero_is_an_error_not_an_infinity() {
        let err = Exact::ONE.div(Exact::ZERO).unwrap_err();
        assert!(matches!(err, RiskError::Arithmetic(_)), "{err}");
    }

    #[test]
    fn one_decimal_matches_the_fixture_wording() {
        let km = |m: i128| Exact::new(m, 1000).unwrap().round_half_even_str(1).unwrap();
        assert_eq!(km(300), "0.3");
        assert_eq!(km(0), "0.0");
        assert_eq!(km(2400), "2.4");
        assert_eq!(km(3100), "3.1");
        assert_eq!(km(1250), "1.2", "half-even baja en el empate");
        assert_eq!(km(1350), "1.4", "half-even sube en el empate");
    }

    #[test]
    fn trimmed_decimal_drops_pointless_zeros() {
        let t = |s: &str| {
            Exact::from_decimal_str(s)
                .unwrap()
                .trimmed_decimal()
                .unwrap()
        };
        assert_eq!(t("3.0"), "3");
        assert_eq!(t("3.5"), "3.5");
        assert_eq!(t("0.25"), "0.25");
        // 3/31 no es decimal finito: mejor fallar que aproximar en silencio.
        assert!(Exact::new(3, 31).unwrap().trimmed_decimal().is_err());
    }

    #[test]
    fn percent_uses_the_fixture_rounding() {
        assert_eq!(bp_to_percent(500), 5);
        assert_eq!(bp_to_percent(5500), 55);
        assert_eq!(bp_to_percent(2050), 21);
        assert_eq!(bp_to_percent(0), 0);
    }

    #[test]
    fn min_and_cmp_do_not_touch_floats() {
        let a = Exact::new(3, 25).unwrap();
        assert_eq!(a.min(Exact::ONE), a);
        assert_eq!(Exact::int(2).min(Exact::ONE), Exact::ONE);
        assert_eq!(a.cmp_exact(&Exact::ONE), Ordering::Less);
    }
}
