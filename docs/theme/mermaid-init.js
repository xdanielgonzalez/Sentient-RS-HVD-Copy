// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

(() => {
    const darkThemes = ['ayu', 'navy', 'coal'];
    const lightThemes = ['light', 'rust'];

    const classList = document.getElementsByTagName('html')[0].classList;

    let lastThemeWasLight = true;
    for (const cssClass of classList) {
        if (darkThemes.includes(cssClass)) {
            lastThemeWasLight = false;
            break;
        }
    }

    const theme = lastThemeWasLight ? 'default' : 'dark';
    mermaid.initialize({ startOnLoad: true, theme });

    // Simplest way to make mermaid re-render the diagrams in the new theme is via refreshing the page.
    // mdbook 0.5 prefixes the theme-picker button ids with "mdbook-theme-"; older versions used the
    // bare theme name — look up both and skip themes absent from the picker.
    const themeButton = (name) =>
        document.getElementById(name) || document.getElementById(`mdbook-theme-${name}`);

    for (const darkTheme of darkThemes) {
        themeButton(darkTheme)?.addEventListener('click', () => {
            if (lastThemeWasLight) {
                window.location.reload();
            }
        });
    }

    for (const lightTheme of lightThemes) {
        themeButton(lightTheme)?.addEventListener('click', () => {
            if (!lastThemeWasLight) {
                window.location.reload();
            }
        });
    }
})();
