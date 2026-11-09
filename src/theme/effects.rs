//! Visual effects themes can opt into.
//!
//! Runtime gating (`Settings::visual_effects`) collapses a theme's set to
//! [`EffectSet::none`] so gloss, shadows, and dim overlays can be switched off.

/// Which optional visuals a theme opts into.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EffectSet {
    /// Horizontal title-bar color ramp (Win98 / Luna / Aero).
    pub title_gradient: bool,
    /// Vertical specular band across the title bar (Luna / Aero glass).
    pub title_gloss: bool,
    /// Soft drop shadow under raised panels / dialogs.
    pub soft_shadow: bool,
    /// Dimmed translucent backdrop behind modal dialogs.
    pub modal_dim: bool,
    /// Light vertical sheen on raised button faces.
    pub button_sheen: bool,
    /// Extra pearl highlight pass on chrome bevel faces (Aero).
    pub glass_chrome: bool,
}

impl EffectSet {
    pub const fn none() -> Self {
        Self {
            title_gradient: false,
            title_gloss: false,
            soft_shadow: false,
            modal_dim: false,
            button_sheen: false,
            glass_chrome: false,
        }
    }

    /// Classic 9x: title gradient only.
    pub const fn classic() -> Self {
        Self {
            title_gradient: true,
            ..Self::none()
        }
    }

    /// Windows XP Luna: gradient + gloss + soft shadows + modal dim + button sheen.
    pub const fn luna() -> Self {
        Self {
            title_gradient: true,
            title_gloss: true,
            soft_shadow: true,
            modal_dim: true,
            button_sheen: true,
            glass_chrome: false,
        }
    }

    /// Windows 7 Aero-ish: glass chrome + gloss + shadows.
    pub const fn aero() -> Self {
        Self {
            title_gradient: true,
            title_gloss: true,
            soft_shadow: true,
            modal_dim: true,
            button_sheen: true,
            glass_chrome: true,
        }
    }

    /// Flat modern: soft shadow + modal dim.
    pub const fn modern() -> Self {
        Self {
            modal_dim: true,
            soft_shadow: true,
            ..Self::none()
        }
    }

    /// Apply the user preference — off means every effect disabled.
    pub const fn gated(self, visual_effects: bool) -> Self {
        if visual_effects {
            self
        } else {
            Self::none()
        }
    }
}
