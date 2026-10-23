import { mount } from "svelte";
import App from "./App.svelte";
import "@fontsource/outfit/latin-400.css";
import "@fontsource/outfit/latin-500.css";
import "@fontsource/outfit/latin-600.css";
import "./styles.css";
import "./preferences.css";
import "./yurelay-design.css";
import "./workflow.css";
import "./onboarding.css";
import "./reading-layout.css";

mount(App, { target: document.getElementById("app")! });
