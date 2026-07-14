//! Built-in UI themes.
//!
//! Built-ins are the only place in `chitin-ui` where concrete color values
//! should be defined. Components consume semantic tokens from [`UIThemes`]
//! instead of hardcoding colors directly.

use super::colors::{lab, laba};
use super::{UIAccentColors, UIBackgroundColors, UIBorderColors, UITextColors, UIThemes};
use tailwind::*;

#[allow(dead_code)]
mod tailwind {
  use super::{lab, laba};

  #[derive(Clone, Copy)]
  pub(super) struct TailwindLab {
    lightness: f32,
    a: f32,
    b: f32,
  }

  impl TailwindLab {
    pub(super) fn hsla(self) -> gpui::Hsla {
      lab(self.lightness, self.a, self.b)
    }

    pub(super) fn hsla_alpha(self, alpha: f32) -> gpui::Hsla {
      laba(self.lightness, self.a, self.b, alpha)
    }
  }

  const fn tailwind_lab(lightness: f32, a: f32, b: f32) -> TailwindLab {
    TailwindLab { lightness, a, b }
  }

  // Tailwind CSS v4 LAB palette tokens, kept private so public consumers depend
  // on semantic theme roles instead of raw palette names.
  pub(super) const RED_50: TailwindLab = tailwind_lab(96.5005, 4.18508, 1.52328);
  pub(super) const RED_100: TailwindLab = tailwind_lab(92.243, 10.2865, 3.83865);
  pub(super) const RED_300: TailwindLab = tailwind_lab(76.5514, 36.422, 15.5335);
  pub(super) const RED_400: TailwindLab = tailwind_lab(63.7053, 60.745, 31.3109);
  pub(super) const RED_500: TailwindLab = tailwind_lab(55.4814, 75.0732, 48.8528);
  pub(super) const RED_600: TailwindLab = tailwind_lab(48.4493, 77.4328, 61.5452);
  pub(super) const RED_700: TailwindLab = tailwind_lab(40.4273, 67.2623, 53.7441);
  pub(super) const RED_800: TailwindLab = tailwind_lab(33.7174, 55.8993, 41.0293);
  pub(super) const RED_900: TailwindLab = tailwind_lab(28.5139, 44.5539, 29.0463);
  pub(super) const RED_950: TailwindLab = tailwind_lab(13.003, 29.04, 16.7519);

  pub(super) const ORANGE_50: TailwindLab = tailwind_lab(97.7008, 1.53735, 5.90649);
  pub(super) const ORANGE_100: TailwindLab = tailwind_lab(94.7127, 3.58394, 14.3151);
  pub(super) const ORANGE_300: TailwindLab = tailwind_lab(80.8059, 21.7313, 50.4455);
  pub(super) const ORANGE_400: TailwindLab = tailwind_lab(70.0429, 42.5156, 75.8207);
  pub(super) const ORANGE_500: TailwindLab = tailwind_lab(64.272, 57.1788, 90.3583);
  pub(super) const ORANGE_600: TailwindLab = tailwind_lab(57.1026, 64.2584, 89.8886);
  pub(super) const ORANGE_700: TailwindLab = tailwind_lab(46.4615, 57.7275, 70.8507);
  pub(super) const ORANGE_800: TailwindLab = tailwind_lab(37.1566, 46.6433, 50.5562);
  pub(super) const ORANGE_900: TailwindLab = tailwind_lab(30.2951, 36.0434, 37.671);

  pub(super) const AMBER_50: TailwindLab = tailwind_lab(98.6252, -0.635922, 8.42309);
  pub(super) const AMBER_200: TailwindLab = tailwind_lab(91.7203, -0.505269, 49.9084);
  pub(super) const AMBER_300: TailwindLab = tailwind_lab(86.4156, 6.13147, 78.3961);
  pub(super) const AMBER_400: TailwindLab = tailwind_lab(80.1641, 16.6016, 99.2089);
  pub(super) const AMBER_500: TailwindLab = tailwind_lab(72.7183, 31.8672, 97.9407);
  pub(super) const AMBER_600: TailwindLab = tailwind_lab(60.3514, 40.5624, 87.1228);
  pub(super) const AMBER_700: TailwindLab = tailwind_lab(47.2709, 42.9082, 69.2966);
  pub(super) const AMBER_800: TailwindLab = tailwind_lab(37.8822, 37.1699, 52.2718);
  pub(super) const AMBER_900: TailwindLab = tailwind_lab(31.2288, 30.2627, 40.0378);
  pub(super) const AMBER_950: TailwindLab = tailwind_lab(15.8111, 20.9107, 23.3752);

  pub(super) const YELLOW_50: TailwindLab = tailwind_lab(98.6846, -1.79055, 9.7766);
  pub(super) const YELLOW_300: TailwindLab = tailwind_lab(89.7033, -0.480294, 84.4917);
  pub(super) const YELLOW_400: TailwindLab = tailwind_lab(83.2664, 8.65132, 106.895);
  pub(super) const YELLOW_500: TailwindLab = tailwind_lab(76.3898, 14.5258, 98.4589);
  pub(super) const YELLOW_600: TailwindLab = tailwind_lab(62.7799, 22.4197, 86.1544);
  pub(super) const YELLOW_700: TailwindLab = tailwind_lab(47.8202, 25.2426, 66.5015);
  pub(super) const YELLOW_800: TailwindLab = tailwind_lab(38.7484, 23.5833, 51.4916);
  pub(super) const YELLOW_900: TailwindLab = tailwind_lab(32.3865, 21.1273, 38.5959);

  pub(super) const LIME_50: TailwindLab = tailwind_lab(98.7039, -5.32573, 10.2149);
  pub(super) const LIME_400: TailwindLab = tailwind_lab(83.7876, -45.0447, 88.4738);
  pub(super) const LIME_500: TailwindLab = tailwind_lab(75.3197, -46.6547, 86.1778);
  pub(super) const LIME_600: TailwindLab = tailwind_lab(61.1055, -41.0235, 73.1483);
  pub(super) const LIME_900: TailwindLab = tailwind_lab(31.9931, -20.7654, 33.7379);

  pub(super) const GREEN_50: TailwindLab = tailwind_lab(98.1563, -5.60117, 2.75915);
  pub(super) const GREEN_100: TailwindLab = tailwind_lab(96.1861, -13.8464, 6.52365);
  pub(super) const GREEN_300: TailwindLab = tailwind_lab(86.9953, -47.2691, 25.0054);
  pub(super) const GREEN_400: TailwindLab = tailwind_lab(78.503, -64.9265, 39.7492);
  pub(super) const GREEN_500: TailwindLab = tailwind_lab(70.5521, -66.5147, 45.8073);
  pub(super) const GREEN_600: TailwindLab = tailwind_lab(59.0978, -58.6621, 41.2579);
  pub(super) const GREEN_700: TailwindLab = tailwind_lab(47.0329, -47.0239, 31.4788);
  pub(super) const GREEN_800: TailwindLab = tailwind_lab(37.4616, -36.7971, 22.9692);
  pub(super) const GREEN_900: TailwindLab = tailwind_lab(30.797, -29.6927, 17.382);
  pub(super) const GREEN_950: TailwindLab = tailwind_lab(15.6845, -20.4225, 11.7249);

  pub(super) const EMERALD_100: TailwindLab = tailwind_lab(94.9004, -17.0769, 5.63836);
  pub(super) const EMERALD_400: TailwindLab = tailwind_lab(75.0771, -60.7313, 19.4147);
  pub(super) const EMERALD_500: TailwindLab = tailwind_lab(66.9756, -58.27, 19.5419);
  pub(super) const EMERALD_600: TailwindLab = tailwind_lab(55.0481, -49.9246, 15.93);
  pub(super) const EMERALD_900: TailwindLab = tailwind_lab(28.8637, -26.9249, 5.45986);

  pub(super) const TEAL_50: TailwindLab = tailwind_lab(98.3189, -4.74921, -0.111711);
  pub(super) const TEAL_300: TailwindLab = tailwind_lab(84.8977, -48.1516, -1.3321);
  pub(super) const TEAL_400: TailwindLab = tailwind_lab(76.0109, -53.3483, -2.27906);
  pub(super) const TEAL_500: TailwindLab = tailwind_lab(67.3859, -49.0983, -2.63511);
  pub(super) const TEAL_600: TailwindLab = tailwind_lab(55.0223, -41.0774, -3.90277);
  pub(super) const TEAL_700: TailwindLab = tailwind_lab(44.4134, -33.1436, -4.22149);
  pub(super) const TEAL_800: TailwindLab = tailwind_lab(35.5975, -26.6648, -4.34487);
  pub(super) const TEAL_900: TailwindLab = tailwind_lab(29.506, -21.4706, -3.59886);

  pub(super) const CYAN_900: TailwindLab = tailwind_lab(30.372, -13.1853, -18.7887);

  pub(super) const SKY_50: TailwindLab = tailwind_lab(97.3623, -2.33802, -4.13098);
  pub(super) const SKY_300: TailwindLab = tailwind_lab(80.3307, -20.2945, -31.385);
  pub(super) const SKY_500: TailwindLab = tailwind_lab(63.3038, -18.433, -51.0407);
  pub(super) const SKY_600: TailwindLab = tailwind_lab(51.7754, -11.4712, -49.8349);
  pub(super) const SKY_700: TailwindLab = tailwind_lab(41.6013, -9.10804, -42.5647);
  pub(super) const SKY_950: TailwindLab = tailwind_lab(17.8299, -5.31271, -21.1584);

  pub(super) const BLUE_50: TailwindLab = tailwind_lab(96.492, -1.14644, -5.11479);
  pub(super) const BLUE_100: TailwindLab = tailwind_lab(92.0301, -2.24757, -11.6453);
  pub(super) const BLUE_200: TailwindLab = tailwind_lab(86.15, -4.04379, -21.0797);
  pub(super) const BLUE_300: TailwindLab = tailwind_lab(77.5052, -6.4629, -36.42);
  pub(super) const BLUE_400: TailwindLab = tailwind_lab(65.0361, -1.42065, -56.9802);
  pub(super) const BLUE_500: TailwindLab = tailwind_lab(54.1736, 13.3369, -74.6839);
  pub(super) const BLUE_600: TailwindLab = tailwind_lab(44.0605, 29.0279, -86.0352);
  pub(super) const BLUE_700: TailwindLab = tailwind_lab(36.9089, 35.0961, -85.6872);
  pub(super) const BLUE_800: TailwindLab = tailwind_lab(30.2514, 27.7853, -70.2699);
  pub(super) const BLUE_900: TailwindLab = tailwind_lab(26.1542, 15.7545, -51.5504);
  pub(super) const BLUE_950: TailwindLab = tailwind_lab(15.6723, 8.86232, -32.2945);

  pub(super) const VIOLET_50: TailwindLab = tailwind_lab(96.2416, 2.28849, -5.51657);
  pub(super) const VIOLET_300: TailwindLab = tailwind_lab(76.7419, 18.3911, -37.0706);
  pub(super) const VIOLET_400: TailwindLab = tailwind_lab(62.8239, 34.9159, -60.0512);
  pub(super) const VIOLET_500: TailwindLab = tailwind_lab(49.9355, 55.1776, -81.8963);
  pub(super) const VIOLET_600: TailwindLab = tailwind_lab(41.088, 68.9966, -91.995);
  pub(super) const VIOLET_700: TailwindLab = tailwind_lab(35.2783, 67.9912, -88.793);
  pub(super) const VIOLET_800: TailwindLab = tailwind_lab(29.3188, 57.7986, -76.1493);
  pub(super) const VIOLET_900: TailwindLab = tailwind_lab(24.3783, 45.7525, -61.4902);

  pub(super) const PURPLE_50: TailwindLab = tailwind_lab(97.1627, 2.99937, -4.13398);
  pub(super) const PURPLE_300: TailwindLab = tailwind_lab(78.3298, 26.2195, -34.9499);
  pub(super) const PURPLE_400: TailwindLab = tailwind_lab(63.6946, 47.6127, -59.2066);
  pub(super) const PURPLE_500: TailwindLab = tailwind_lab(52.0183, 66.11, -78.2316);
  pub(super) const PURPLE_600: TailwindLab = tailwind_lab(43.0295, 75.21, -86.5669);
  pub(super) const PURPLE_700: TailwindLab = tailwind_lab(36.1758, 69.8525, -80.0381);
  pub(super) const PURPLE_800: TailwindLab = tailwind_lab(30.6017, 56.7637, -64.4751);
  pub(super) const PURPLE_900: TailwindLab = tailwind_lab(24.9401, 45.2703, -51.2728);
  pub(super) const PURPLE_950: TailwindLab = tailwind_lab(14.8253, 38.9005, -44.5861);

  pub(super) const PINK_400: TailwindLab = tailwind_lab(64.5597, 64.3615, -12.7988);
  pub(super) const PINK_500: TailwindLab = tailwind_lab(56.9303, 76.8162, -8.07021);
  pub(super) const PINK_600: TailwindLab = tailwind_lab(49.5493, 79.8381, 2.31768);

  pub(super) const ROSE_50: TailwindLab = tailwind_lab(96.2369, 4.94155, 1.28011);
  pub(super) const ROSE_300: TailwindLab = tailwind_lab(76.6339, 38.3549, 9.68835);
  pub(super) const ROSE_400: TailwindLab = tailwind_lab(64.4125, 63.0291, 19.2068);
  pub(super) const ROSE_500: TailwindLab = tailwind_lab(56.101, 79.4328, 31.4532);
  pub(super) const ROSE_600: TailwindLab = tailwind_lab(49.1882, 81.577, 36.0311);
  pub(super) const ROSE_700: TailwindLab = tailwind_lab(41.1651, 71.6251, 30.3087);
  pub(super) const ROSE_800: TailwindLab = tailwind_lab(34.6481, 60.802, 20.1957);
  pub(super) const ROSE_900: TailwindLab = tailwind_lab(29.7104, 51.514, 12.6253);

  pub(super) const SLATE_50: TailwindLab = tailwind_lab(98.1434, -0.369519, -1.05966);
  pub(super) const SLATE_100: TailwindLab = tailwind_lab(96.286, -0.852436, -2.46847);
  pub(super) const SLATE_200: TailwindLab = tailwind_lab(91.7353, -0.998765, -4.76968);
  pub(super) const SLATE_400: TailwindLab = tailwind_lab(65.5349, -2.25151, -14.5072);
  pub(super) const SLATE_500: TailwindLab = tailwind_lab(48.0876, -2.03595, -16.5814);
  pub(super) const SLATE_800: TailwindLab = tailwind_lab(16.132, -0.318035, -14.6672);
  pub(super) const SLATE_900: TailwindLab = tailwind_lab(7.78673, 1.82345, -15.0537);
  pub(super) const SLATE_950: TailwindLab = tailwind_lab(1.76974, 1.32743, -9.28855);

  pub(super) const GRAY_50: TailwindLab = tailwind_lab(98.2596, -0.247031, -0.706708);
  pub(super) const GRAY_100: TailwindLab = tailwind_lab(96.1596, -0.0823438, -1.13575);
  pub(super) const GRAY_200: TailwindLab = tailwind_lab(91.6229, -0.159115, -2.26791);
  pub(super) const GRAY_400: TailwindLab = tailwind_lab(65.9269, -0.832707, -8.17473);
  pub(super) const GRAY_500: TailwindLab = tailwind_lab(47.7841, -0.393182, -10.0268);
  pub(super) const GRAY_700: TailwindLab = tailwind_lab(27.1134, -0.956401, -12.3224);
  pub(super) const GRAY_800: TailwindLab = tailwind_lab(16.1051, -1.18239, -11.7533);
  pub(super) const GRAY_900: TailwindLab = tailwind_lab(8.11897, 0.811279, -12.254);
  pub(super) const GRAY_950: TailwindLab = tailwind_lab(1.90334, 0.278696, -5.48866);

  pub(super) const ZINC_50: TailwindLab = tailwind_lab(98.26, 0.0, 0.0);
  pub(super) const ZINC_100: TailwindLab = tailwind_lab(96.1634, 0.0993311, -0.364041);
  pub(super) const ZINC_200: TailwindLab = tailwind_lab(90.6853, 0.399232, -1.45452);
  pub(super) const ZINC_400: TailwindLab = tailwind_lab(65.6464, 1.53497, -5.42429);
  pub(super) const ZINC_500: TailwindLab = tailwind_lab(47.8878, 1.65477, -5.77283);
  pub(super) const ZINC_800: TailwindLab = tailwind_lab(15.7305, 0.613764, -2.16959);
  pub(super) const ZINC_900: TailwindLab = tailwind_lab(8.30603, 0.618205, -2.16572);
  pub(super) const ZINC_950: TailwindLab = tailwind_lab(2.51107, 0.242703, -0.886115);

  pub(super) const NEUTRAL_50: TailwindLab = tailwind_lab(98.26, 0.0, 0.0);
  pub(super) const NEUTRAL_100: TailwindLab = tailwind_lab(96.52, -0.0000298023, 0.0000119209);
  pub(super) const NEUTRAL_200: TailwindLab = tailwind_lab(90.952, 0.0, -0.0000119209);
  pub(super) const NEUTRAL_300: TailwindLab = tailwind_lab(84.92, 0.0, -0.0000119209);
  pub(super) const NEUTRAL_400: TailwindLab = tailwind_lab(66.128, -0.0000298023, 0.0000119209);
  pub(super) const NEUTRAL_500: TailwindLab = tailwind_lab(48.496, 0.0, 0.0);
  pub(super) const NEUTRAL_600: TailwindLab = tailwind_lab(34.924, 0.0, 0.0);
  pub(super) const NEUTRAL_700: TailwindLab = tailwind_lab(27.036, 0.0, 0.0);
  pub(super) const NEUTRAL_800: TailwindLab = tailwind_lab(15.204, 0.0, -0.00000596046);
  pub(super) const NEUTRAL_900: TailwindLab = tailwind_lab(7.78201, -0.0000149012, 0.0);
  pub(super) const NEUTRAL_950: TailwindLab = tailwind_lab(2.75381, 0.0, 0.0);

  pub(super) const STONE_50: TailwindLab = tailwind_lab(98.2686, -0.0991821, 0.364304);
  pub(super) const STONE_100: TailwindLab = tailwind_lab(96.5286, -0.0991821, 0.364268);
  pub(super) const STONE_200: TailwindLab = tailwind_lab(91.055, 0.663072, 0.865579);
  pub(super) const STONE_300: TailwindLab = tailwind_lab(84.7909, 0.928015, 1.59738);
  pub(super) const STONE_400: TailwindLab = tailwind_lab(66.2166, 1.88044, 3.20326);
  pub(super) const STONE_500: TailwindLab = tailwind_lab(48.1164, 2.35701, 4.26852);
  pub(super) const STONE_600: TailwindLab = tailwind_lab(35.5168, 1.08604, 4.07829);
  pub(super) const STONE_700: TailwindLab = tailwind_lab(27.3812, 1.32917, 3.57789);
  pub(super) const STONE_800: TailwindLab = tailwind_lab(15.0353, 1.96067, 1.53427);
  pub(super) const STONE_900: TailwindLab = tailwind_lab(9.03835, 1.15298, 1.92955);
  pub(super) const STONE_950: TailwindLab = tailwind_lab(2.86037, 0.455312, 0.568903);
}

/// Returns the default dark UI theme.
///
/// The palette follows Tailwind CSS v4 LAB neutral colors, adapted to Chitin's
/// current semantic token groups. It is intentionally quiet and IDE-oriented:
/// dark neutral surfaces, muted secondary text, clear selected states, and a
/// restrained blue accent for active navigation.
pub fn dark() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: NEUTRAL_50.hsla(),
      secondary: NEUTRAL_400.hsla(),
      disabled: NEUTRAL_500.hsla(),
      hover: NEUTRAL_50.hsla(),
      selection: NEUTRAL_50.hsla(),
      highlight: AMBER_400.hsla(),
      error: RED_500.hsla(),
      warning: AMBER_400.hsla(),
      info: BLUE_500.hsla(),
      success: GREEN_500.hsla(),
    },
    background: UIBackgroundColors {
      primary: NEUTRAL_800.hsla(),
      secondary: NEUTRAL_950.hsla(),
      hover: NEUTRAL_700.hsla(),
      active: NEUTRAL_700.hsla(),
      selection: NEUTRAL_700.hsla(),
      error: RED_500.hsla(),
      warning: AMBER_400.hsla(),
      info: BLUE_500.hsla(),
      success: GREEN_500.hsla(),
    },
    border: UIBorderColors {
      primary: NEUTRAL_50.hsla_alpha(0.1),
      muted: NEUTRAL_50.hsla_alpha(0.08),
      focus: NEUTRAL_500.hsla(),
    },
    accent: UIAccentColors {
      primary: BLUE_600.hsla(),
      foreground: NEUTRAL_50.hsla(),
    },
  }
}

/// Returns the default light UI theme.
///
/// This is the light counterpart to [`dark`], using Tailwind CSS v4 LAB neutral
/// defaults adapted to Chitin's current semantic token groups.
pub fn light() -> UIThemes {
  UIThemes {
    text: UITextColors {
      primary: NEUTRAL_950.hsla(),
      secondary: NEUTRAL_500.hsla(),
      disabled: NEUTRAL_400.hsla(),
      hover: NEUTRAL_950.hsla(),
      selection: NEUTRAL_800.hsla(),
      highlight: AMBER_500.hsla(),
      error: RED_600.hsla(),
      warning: AMBER_500.hsla(),
      info: BLUE_600.hsla(),
      success: GREEN_600.hsla(),
    },
    background: UIBackgroundColors {
      primary: NEUTRAL_50.hsla(),
      secondary: NEUTRAL_100.hsla(),
      hover: NEUTRAL_100.hsla(),
      active: NEUTRAL_200.hsla(),
      selection: NEUTRAL_100.hsla(),
      error: RED_600.hsla(),
      warning: AMBER_500.hsla(),
      info: BLUE_600.hsla(),
      success: GREEN_600.hsla(),
    },
    border: UIBorderColors {
      primary: NEUTRAL_200.hsla(),
      muted: NEUTRAL_100.hsla(),
      focus: NEUTRAL_400.hsla(),
    },
    accent: UIAccentColors {
      primary: NEUTRAL_800.hsla(),
      foreground: NEUTRAL_50.hsla(),
    },
  }
}
