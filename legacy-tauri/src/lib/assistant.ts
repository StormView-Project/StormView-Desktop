import { format } from 'date-fns';
import { api } from './api';
import { currentHourIndex, num } from './format';
import { $ } from './store';
import type { ChatMessage, WeatherData } from './types';

// Keeps prompts inside small local models' context windows, and stops old
// questions from crowding out the current one.
const MAX_HISTORY = 8;

export const WEBLLM_MODELS = [
  { id: 'Llama-3.2-1B-Instruct-q4f16_1-MLC', label: 'Llama 3.2 1B (fast, ~0.9 GB)' },
  { id: 'Qwen2.5-1.5B-Instruct-q4f16_1-MLC', label: 'Qwen2.5 1.5B (better answers, ~1.3 GB)' },
];

function dayLabel(date: number, i: number) {
  if (i === 0) return 'Today';
  if (i === 1) return 'Tomorrow';
  return format(date, 'EEEE');
}

/** Condenses live weather into a system prompt; one fact per line so small models don't mix numbers up. */
export function buildWeatherContext(w: WeatherData | null): string {
  if (!w) return 'No weather data is currently loaded for the user.';
  const s = $();
  const tu = s.units === 'imperial' ? '°F' : '°C';
  const su = s.units === 'imperial' ? 'mph' : 'km/h';
  const cu = w.current;
  const lines: string[] = [`Today's date: ${format(Date.now(), 'EEEE, MMMM d')}`];
  if (s.location) lines.push(`Location: ${s.location.name}${s.location.state ? `, ${s.location.state}` : ''}`);
  lines.push(
    `Current conditions right now: ${cu.condition}, temperature ${num(cu.temp)}${tu}` +
      (cu.feelsLike != null ? `, feels like ${num(cu.feelsLike)}${tu}` : '') +
      (cu.humidity != null ? `, humidity ${cu.humidity}%` : '') +
      (cu.windSpeed != null ? `, wind ${num(cu.windSpeed)} ${su}` : '') +
      (cu.uvIndex != null ? `, UV index ${cu.uvIndex}` : ''),
  );
  const start = currentHourIndex(w.hourly);
  const hours = w.hourly.slice(start, start + 8);
  if (hours.length) {
    lines.push('', 'Hour-by-hour (starting from the current hour):');
    for (const h of hours) {
      lines.push(`- ${format(h.time, 'h a')}: ${num(h.temp)}${tu}${h.precipProb != null ? `, ${h.precipProb}% chance of precipitation` : ''}`);
    }
  }
  if (w.daily.length) {
    lines.push('', 'Daily forecast — use these exact numbers, do not recompute or guess new ones:');
    w.daily.slice(0, 7).forEach((d, i) => {
      lines.push(
        `- ${dayLabel(d.date, i)} (${format(d.date, 'MMM d')}): high ${num(d.tempMax)}${tu}, low ${num(d.tempMin)}${tu}` +
          (d.condition ? `, ${d.condition}` : '') +
          (d.precipProb != null ? `, ${d.precipProb}% chance of precipitation` : ''),
      );
    });
  }
  const alerts = s.alerts.length ? s.alerts : w.alerts;
  lines.push('', alerts.length ? 'Active alerts: ' + alerts.map((a) => `${a.title} (${a.severity})`).join('; ') : 'No active weather alerts.');
  if (w.airQuality?.us_aqi != null) lines.push(`Air quality: US AQI ${w.airQuality.us_aqi}`);
  return lines.join('\n');
}

export function systemPrompt(w: WeatherData | null) {
  return (
    "You are StormView's weather assistant, embedded in a weather radar app.\n" +
    'Rules:\n' +
    "1. Read the user's question carefully and answer ONLY that question. If they ask about precipitation, " +
    'answer about precipitation — not temperature, wind, or anything else, even if you discussed something else earlier.\n' +
    "2. Use ONLY the numbers in the data below — never invent, guess, or recompute a number that isn't already there.\n" +
    '3. Be concise and conversational.\n\n' +
    buildWeatherContext(w)
  );
}

/* ------------------------------------------------------------- WebLLM path */

type Engine = {
  chat: {
    completions: {
      create: (o: object) => Promise<AsyncIterable<{ choices: { delta: { content?: string } }[] }>>;
    };
  };
};
let enginePromise: Promise<Engine> | null = null;
let engineModel = '';

export async function webgpuAvailable(): Promise<boolean> {
  const gpu = (navigator as unknown as { gpu?: { requestAdapter: () => Promise<unknown> } }).gpu;
  if (!gpu) return false;
  try {
    const adapter = await Promise.race([gpu.requestAdapter(), new Promise((r) => setTimeout(() => r(null), 6000))]);
    return !!adapter;
  } catch {
    return false;
  }
}

export async function loadWebLLM(model: string, onProgress: (p: number, text: string) => void) {
  if (enginePromise && engineModel === model) return enginePromise;
  const webllm = await import('@mlc-ai/web-llm');
  engineModel = model;
  enginePromise = webllm.CreateMLCEngine(model, {
    initProgressCallback: (r: { progress: number; text: string }) => onProgress(r.progress, r.text),
  }) as unknown as Promise<Engine>;
  try {
    return await enginePromise;
  } catch (e) {
    enginePromise = null;
    throw e;
  }
}

export async function clearWebLLMCache() {
  const webllm = await import('@mlc-ai/web-llm');
  await Promise.all(WEBLLM_MODELS.map((m) => webllm.deleteModelAllInfoInCache(m.id).catch(() => {})));
  enginePromise = null;
}

/* ------------------------------------------------------------------ chat */

export async function sendChat(history: ChatMessage[], onDelta: (t: string) => void) {
  const s = $();
  const messages: ChatMessage[] = [{ role: 'system', content: systemPrompt(s.weather) }, ...history.slice(-MAX_HISTORY)];
  if (s.assistantProvider === 'webllm') {
    const engine = await loadWebLLM(s.assistantModel || WEBLLM_MODELS[0].id, () => {});
    const stream = await engine.chat.completions.create({ messages, stream: true });
    for await (const chunk of stream) {
      const d = chunk.choices?.[0]?.delta?.content;
      if (d) onDelta(d);
    }
    return;
  }
  if (!s.assistantModel) throw new Error('Pick a model in Settings → Assistant first.');
  await api.assistantChat(s.assistantProvider, s.assistantBaseUrl, s.assistantModel, messages, onDelta);
}
