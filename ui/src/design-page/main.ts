import '../lib/design/design.css';

import { mount } from 'svelte';

import DesignPage from './DesignPage.svelte';

const target = document.getElementById('app');
if (!target) throw new Error('design.html has no #app element');

mount(DesignPage, { target });
