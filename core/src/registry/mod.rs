use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverageStatus {
    Protected,
    Partial,
    Uncovered,
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub aliases: Vec<String>,
    pub provider_id: String,
}

#[derive(Debug, Clone)]
pub struct ProviderInfo {
    pub id: String,
    pub upstream_url: String,
    pub schedule_id: String,
    pub enabled: bool,
}

pub struct ModelRegistry {
    providers: HashMap<String, ProviderInfo>,
    models: HashMap<String, ModelInfo>,
    alias_to_model: HashMap<String, String>,
}

impl Default for ModelRegistry {
    fn default() -> Self {
        let mut registry = Self {
            providers: HashMap::new(),
            models: HashMap::new(),
            alias_to_model: HashMap::new(),
        };

        // Proveedor por defecto 1: DeepSeek Direct API
        registry.register_provider(ProviderInfo {
            id: "deepseek_direct".to_string(),
            upstream_url: "https://api.deepseek.com".to_string(),
            schedule_id: "deepseek_direct_verified".to_string(),
            enabled: true,
        });

        registry.register_model(ModelInfo {
            id: "deepseek-chat".to_string(),
            name: "DeepSeek-V3 Chat".to_string(),
            aliases: vec![
                "deepseek/deepseek-chat".to_string(),
                "deepseek-v3".to_string(),
            ],
            provider_id: "deepseek_direct".to_string(),
        });

        registry.register_model(ModelInfo {
            id: "deepseek-reasoner".to_string(),
            name: "DeepSeek-R1 Reasoner".to_string(),
            aliases: vec![
                "deepseek/deepseek-reasoner".to_string(),
                "deepseek-r1".to_string(),
            ],
            provider_id: "deepseek_direct".to_string(),
        });

        // Proveedor por defecto 2: OpenCode Go (Perfil independiente)
        registry.register_provider(ProviderInfo {
            id: "opencode_go".to_string(),
            upstream_url: "https://opencode.ai/zen/go/v1".to_string(),
            schedule_id: "opencode_go_verified".to_string(),
            enabled: false,
        });

        registry.register_model(ModelInfo {
            id: "deepseek-v4.1-flash".to_string(),
            name: "DeepSeek V4.1 Flash (Go)".to_string(),
            aliases: vec![
                "opencode/deepseek-v4-flash-free".to_string(),
                "opencode-go/deepseek-v4-flash".to_string(),
            ],
            provider_id: "opencode_go".to_string(),
        });

        registry
    }
}

impl ModelRegistry {
    pub fn register_provider(&mut self, provider: ProviderInfo) {
        self.providers.insert(provider.id.clone(), provider);
    }

    pub fn register_model(&mut self, model: ModelInfo) {
        let model_id = model.id.clone();
        for alias in &model.aliases {
            self.alias_to_model
                .insert(alias.to_lowercase(), model_id.clone());
        }
        self.alias_to_model
            .insert(model_id.to_lowercase(), model_id.clone());
        self.models.insert(model_id, model);
    }

    /// Resuelve el modelo canónico a partir del nombre o alias recibido en la petición
    pub fn resolve_model(&self, model_name: &str) -> Option<&ModelInfo> {
        let normalized = model_name.trim().to_lowercase();
        let canonical_id = self.alias_to_model.get(&normalized)?;
        self.models.get(canonical_id)
    }

    /// Obtiene información del proveedor asociado a un modelo
    pub fn get_provider_for_model(&self, model_name: &str) -> Option<&ProviderInfo> {
        let model = self.resolve_model(model_name)?;
        self.providers.get(&model.provider_id)
    }

    /// Clasifica el estado de cobertura del modelo solicitado
    pub fn classify_coverage(&self, model_name: &str, passes_through_gateway: bool) -> CoverageStatus {
        if !passes_through_gateway {
            return CoverageStatus::Uncovered;
        }

        match self.get_provider_for_model(model_name) {
            Some(provider) if provider.enabled => CoverageStatus::Protected,
            Some(_) => CoverageStatus::Partial,
            None => CoverageStatus::Uncovered,
        }
    }
}
