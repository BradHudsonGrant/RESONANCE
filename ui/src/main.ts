import './lib/design/design.css';

import { mount } from 'svelte';

import App from './App.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('index.html has no #app element');

mount(App, { target });

if (import.meta.env.DEV) {
  void import('./lib/library/measure-scroll').then(({ measureTableScroll }) => {
    Object.assign(window, { measureTableScroll });
  });
}

if (import.meta.env.MODE === 'benchmark') {
  void import('../dev/ScrollBench.svelte').then(({ default: ScrollBench }) => {
    mount(ScrollBench, { target: document.body });
  });
}
