/** @type {import('tailwindcss').Config} */
module.exports = {
  mode: "all",
  content: [
    // include all rust, html and css files in the src directory
    "./src/**/*.{rs,html,css}",
    // include all html files in the output (dist) directory
    "./dist/**/*.html",
    // include the custom index file
    "./index.html",
  ],
  theme: {
    extend: {},
  },
  plugins: [],
}

