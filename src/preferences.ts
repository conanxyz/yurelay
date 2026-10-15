import type {Settings} from './types';
export type DisplayPreferences=Pick<Settings,'target_language'|'ui_language'|'theme'|'ui_font_size'|'font_size'|'auto_hide'|'max_concurrency'>;
export function displayPreferences(s:Settings):DisplayPreferences {
 return {target_language:s.target_language,ui_language:s.ui_language,theme:s.theme,ui_font_size:s.ui_font_size,font_size:s.font_size,auto_hide:s.auto_hide,max_concurrency:s.max_concurrency};
}
