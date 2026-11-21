/** Paleta Radiant/Dire heredada de v3.0. @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.ts"],
  theme: {
    extend: {
      colors: {
        orion: {
          bg: "#020408",
          panel: "rgba(7, 11, 20, 0.92)",
          panelDark: "rgba(3, 6, 12, 0.96)",
          border: "#d4af37",
          radiant: "#00ffa3",
          dire: "#ff2a4b",
          river: "#00c3ff",
          gold: "#ecc94b",
          steam: "#66c0f4",
          amber: "#ff9900",
          purple: "#b832ff",
        },
      },
      fontFamily: {
        orbitron: ["Orbitron", "sans-serif"],
        rajdhani: ["Rajdhani", "sans-serif"],
        mono: ["'Share Tech Mono'", "monospace"],
        cinzel: ["Cinzel", "serif"],
      },
    },
  },
  plugins: [],
};
