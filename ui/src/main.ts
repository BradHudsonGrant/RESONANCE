import './lib/design/design.css';
import './lib/design/theme';

import { mount } from 'svelte';

setTimeout(() => {
  (1000).toLocaleString('en-GB');
});

const target = document.getElementById('app');
if (!target) throw new Error('index.html has no #app element');

// Loading and mounting after parsing finishes keeps script evaluation, first render and layout
// in separate main-thread tasks.
void import('./App.svelte').then(({ default: App }) => {
  setTimeout(() => {
    mount(App, { target });
  });
});

// `resonance bench` calls these in the desktop app, which runs the benchmark build.
if (import.meta.env.DEV || import.meta.env.MODE === 'benchmark') {
  setTimeout(() => {
    void import('./lib/library/measure-scroll').then(({ measureTableScroll }) => {
      Object.assign(window, { measureTableScroll });
    });
    void import('./lib/wave/measure-wave').then(({ measureWave }) => {
      Object.assign(window, { measureWave });
    });
    void import('./lib/mix/measure-mix').then(({ measureMix }) => {
      Object.assign(window, { measureMix });
    });
    void import('./lib/shell/measure-auto-preview').then(({ measureAutoPreview }) => {
      Object.assign(window, { measureAutoPreview });
    });
  }, 250);
}

if (import.meta.env.MODE === 'benchmark') {
  setTimeout(() => {
    void import('../dev/ScrollBench.svelte').then(({ default: ScrollBench }) => {
      mount(ScrollBench, { target: document.body });
    });
  }, 300);
}
