import React from 'react';
import ReactDOM from 'react-dom/client';

import App from './App';
import './styles.css';
import { initVideoSrc } from './videoSrc';

// L'adresse des vidéos doit être connue avant le premier `<video>`.
void initVideoSrc().then(() =>
  ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  ),
);
