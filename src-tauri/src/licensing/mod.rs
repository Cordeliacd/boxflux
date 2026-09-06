//! Sistema de licencias.
//!
//! - `LicenseProvider` trait: plugable (local ahora, verificador remoto
//!   en el futuro).
//! - `LicenseValidator` trait: gate de features.
//! - `LicenseManager`: facade que orquesta provider + validator.
//!
//! El provider local actual es dev-only; la verificación offline de
//! claves Ed25519 (clave pública embebida en el binario) está pendiente.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

/// Tier de licencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum LicenseTier {
    #[default]
    Free,
    Standard,
    Pro,
    Team,
}

impl LicenseTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            LicenseTier::Free => "Free",
            LicenseTier::Standard => "Standard",
            LicenseTier::Pro => "Pro",
            LicenseTier::Team => "Team",
        }
    }
}

/// Estado de validación de la licencia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub enum LicenseStatus {
    #[default]
    Unknown,
    Valid,
    Expired,
    Revoked,
    Invalid,
}

impl LicenseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            LicenseStatus::Unknown => "Unknown",
            LicenseStatus::Valid => "Valid",
            LicenseStatus::Expired => "Expired",
            LicenseStatus::Revoked => "Revoked",
            LicenseStatus::Invalid => "Invalid",
        }
    }
}

/// Entitlement completo de un producto. Resultado de activar una clave.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProductEntitlement {
    pub product_id: String,
    pub product_name: String,
    #[serde(default)]
    pub tier: LicenseTier,
    #[serde(default)]
    pub status: LicenseStatus,
    #[serde(default)]
    pub owner_email: String,
    #[serde(default)]
    pub issued_at: String,
    #[serde(default)]
    pub expires_at: String,
    #[serde(default)]
    pub is_trial: bool,
    #[serde(default)]
    pub is_commercial_use_allowed: bool,
}

/// Provider de licencias: sabe obtener la entitlement actual y activar
/// una nueva clave.
pub trait LicenseProvider: Send + Sync {
    fn current_entitlement(&self) -> ProductEntitlement;
    fn activate_with_key(&self, key: &str) -> Result<ProductEntitlement, String>;
    fn deactivate(&self);
    fn provider_name(&self) -> &str;
}

/// Validator: decide qué features están permitidas según la entitlement.
pub trait LicenseValidator: Send + Sync {
    fn is_entitled_to(&self, e: &ProductEntitlement, feature: &str) -> bool;
    fn validate(&self, e: &ProductEntitlement) -> LicenseStatus;
}

/// Feature gates:
/// - `optimize` → siempre true (Free+)
/// - `convert`, `batch` → Standard+
/// - `avif` → Pro+
/// - `commercial_use` → flag o Team+
pub struct DefaultLicenseValidator;

impl LicenseValidator for DefaultLicenseValidator {
    fn is_entitled_to(&self, e: &ProductEntitlement, feature: &str) -> bool {
        match feature {
            "optimize" => true,
            "convert" | "batch" => e.tier >= LicenseTier::Standard,
            "avif" => e.tier >= LicenseTier::Pro,
            "commercial_use" => e.is_commercial_use_allowed || e.tier >= LicenseTier::Team,
            _ => false,
        }
    }

    fn validate(&self, e: &ProductEntitlement) -> LicenseStatus {
        if e.status == LicenseStatus::Unknown {
            // Si la entitlement está vacía, la consideramos Free/Valid por defecto.
            return LicenseStatus::Valid;
        }
        e.status
    }
}

/// Provider local dev-only. Siempre devuelve Free / Valid.
/// En producción se sustituirá por uno que verifique firmas Ed25519
/// offline (clave pública embebida).
pub struct LocalLicenseProvider {
    inner: RwLock<ProductEntitlement>,
}

impl LocalLicenseProvider {
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(ProductEntitlement {
                product_id: "boxflux".into(),
                product_name: "BoxFlux".into(),
                tier: LicenseTier::Free,
                status: LicenseStatus::Valid,
                ..Default::default()
            }),
        }
    }
}

impl Default for LocalLicenseProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LicenseProvider for LocalLicenseProvider {
    fn current_entitlement(&self) -> ProductEntitlement {
        self.inner.read().clone()
    }

    fn activate_with_key(&self, key: &str) -> Result<ProductEntitlement, String> {
        if key.trim().is_empty() {
            return Err("clave vacía".into());
        }
        // Dev-only: acepta cualquier clave no vacía como Standard.
        let mut e = self.inner.write();
        *e = ProductEntitlement {
            product_id: "boxflux".into(),
            product_name: "BoxFlux".into(),
            tier: LicenseTier::Standard,
            status: LicenseStatus::Valid,
            owner_email: String::new(),
            issued_at: chrono::Utc::now().to_rfc3339(),
            expires_at: String::new(),
            is_trial: false,
            is_commercial_use_allowed: false,
        };
        Ok(e.clone())
    }

    fn deactivate(&self) {
        let mut e = self.inner.write();
        *e = ProductEntitlement {
            product_id: "boxflux".into(),
            product_name: "BoxFlux".into(),
            tier: LicenseTier::Free,
            status: LicenseStatus::Valid,
            ..Default::default()
        };
    }

    fn provider_name(&self) -> &str {
        "Local"
    }
}

/// Manager: facade sobre provider + validator.
pub struct LicenseManager {
    provider: Arc<dyn LicenseProvider>,
    validator: Arc<dyn LicenseValidator>,
}

impl LicenseManager {
    pub fn new(provider: Arc<dyn LicenseProvider>, validator: Arc<dyn LicenseValidator>) -> Self {
        Self {
            provider,
            validator,
        }
    }

    pub fn with_local_provider() -> Self {
        Self::new(
            Arc::new(LocalLicenseProvider::new()),
            Arc::new(DefaultLicenseValidator),
        )
    }

    pub fn entitlement(&self) -> ProductEntitlement {
        self.provider.current_entitlement()
    }

    pub fn activate_with_key(&self, key: &str) -> Result<ProductEntitlement, String> {
        self.provider.activate_with_key(key)
    }

    pub fn deactivate(&self) {
        self.provider.deactivate();
    }

    pub fn is_entitled_to(&self, feature: &str) -> bool {
        let e = self.provider.current_entitlement();
        self.validator.is_entitled_to(&e, feature)
    }

    pub fn status(&self) -> LicenseStatus {
        let e = self.provider.current_entitlement();
        self.validator.validate(&e)
    }

    pub fn tier(&self) -> LicenseTier {
        self.provider.current_entitlement().tier
    }
}
