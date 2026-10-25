;(function () {
  try {
    var stored = window.localStorage.getItem('orbit.theme')
    var systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches
    var theme = stored === 'light' || stored === 'dark' ? stored : systemDark ? 'dark' : 'light'
    var root = document.documentElement
    root.setAttribute('data-theme', theme)
    root.classList.toggle('dark', theme === 'dark')
    root.style.colorScheme = theme
    var meta = document.querySelector('meta[name="theme-color"]')
    if (meta) meta.setAttribute('content', theme === 'dark' ? '#121318' : '#f7f5f0')
  } catch (error) {
    /* Storage can be unavailable; the app still works in the default theme. */
  }
})()
