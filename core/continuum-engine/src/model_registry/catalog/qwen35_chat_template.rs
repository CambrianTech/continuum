use super::*;

pub(super) const QWEN35_CHAT_TEMPLATE: &str = "{% for message in messages %}{{ '<|im_start|>' + message['role'] + '\\n' + message['content'] + '<|im_end|>\\n' }}{% endfor %}{% if add_generation_prompt %}{{ '<|im_start|>assistant\\n' }}{% endif %}";

pub fn registry() -> Result<Registry, RegistryError> {
    Registry::from_catalog(models(), providers())
}
