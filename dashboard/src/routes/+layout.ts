// The appliance has no Node runtime: the daemon serves static files and the
// app runs entirely in the browser, talking to the API with its session
// cookie. Disabling SSR is what makes that a single-page application.
export const ssr = false;
