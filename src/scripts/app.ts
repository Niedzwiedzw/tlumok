// ==UserScript==
// @name         Turbo translate
// @namespace    Turbo translate
// @version      0.1
// @description  Fast translation with google translate
// @author       Niedzwiedzw
// @match        https://translate.google.com/
// @grant        none
// ==/UserScript==

import { isNil, trim, endsWith } from 'lodash';

function getRandomArbitrary(min: number, max: number): number {
  return Math.ceil(Math.random() * (max - min) + min);
}

class TurboTranslator {
  BUTTON_SELECTOR = '#turbo-translate-button';
  INTERVAL = 200; // ms
  SENTENCE_REGEX = /[^\.!\?]+[\.!\?]+/g;
  lastClipboard = '';
  intervalHandlers: number[] = [];

  public constructor() {
    console.log(this.BUTTON_SELECTOR);
    console.log(this.button);
    let button = this.button;
    if (isNil(button)) {
      console.log('button not present, creating one');
      button = this.createButton();
    }
    this.listen(button as HTMLButtonElement);
  }

  private stopLooping() {
    for (let handler of this.intervalHandlers) {
      window.clearInterval(handler);
    }
    this.intervalHandlers = [];
  }

  private listen(button: HTMLButtonElement) {
    button!.addEventListener('click', () => this.run());
  }

  private get button() {
    return document.querySelector(this.BUTTON_SELECTOR);
  }

  private get leftInput(): HTMLTextAreaElement {
    const input = document.querySelector('#source');
    if (isNil(input)) {
      throw Error('Could not find textarea...');
    }
    return input as HTMLTextAreaElement;
  }

  private get copyButton(): HTMLElement | null {
    return document.querySelector('.tild-copy-translation-button');
  }

  private createButton() {
    const button = document.createElement('button');
    button.innerHTML = 'START';
    button.id = this.BUTTON_SELECTOR;
    document.body.appendChild(button);
    return button;
  }

  private async run() {
    this.intervalHandlers.push(window.setInterval(() => this.performTranslation(), this.INTERVAL));
  }

  private shouldPerform(text: string): boolean {
    return this.lastClipboard !== text;
  }

  private async withPauseClipboard(procedure: () => Promise<void>) {
    this.stopLooping();
    await procedure();
    this.run();
  }

  private async performTranslation() {
    let text = '';
    try {
      text = await navigator.clipboard.readText();
    } catch (e) {
      return;
    }
    if (!this.shouldPerform(text)) {
      return;
    }
    await this.withPauseClipboard(async () => {
      console.log('calling for a translation');
      let translation = await this.translate(text);
      this.lastClipboard = translation;
      console.log(`i got it! ${translation}`);
      navigator.clipboard.writeText(translation);
    });
  }

  private async setInput(text: string) {
    this.leftInput.click();
    this.leftInput.value = text;
    this.leftInput.click();
    this.leftInput.click();
    this.leftInput.click();
  }

  private async translateChunk(text: string): Promise<string> {
    await this.resetInput();
    await this.setInput(text);
    return await this.newTranslation();
  }

  private async chunks(text: string, maxSize = 4999): Promise<string[]> {
    if (text.length <= maxSize) {
      return [text];
    }
    const sentences = text.match(this.SENTENCE_REGEX);
    if (isNil(sentences)) {
      throw Error('Failed to split into sentences, check regex!');
    }
    const chunks = [];

    let chunk = '';
    for (const sentence of sentences) {
      if (chunk.length + sentence.length > maxSize) {
        chunks.push(chunk);
        chunk = '';
      }
      chunk += sentence;
    }

    return chunks;
  }

  private async newTranslation(): Promise<string> {
    const previous = this.translationText;
    await this.translationValueIsNot(previous);
    return this.translationText!;
  }

  private wait(timeMs: number): Promise<void> {
    return new Promise((resolve, reject) => {
      setTimeout(resolve, timeMs);
    });
  }

  private async translationValuePassesTest(test: (value: string | null) => boolean): Promise<void> {
    return new Promise((resolve, reject) => {
      const handler = setInterval(() => {
        const val = this.translationText;
        if (val === null) {
          console.error('translation text is null... continuing but... this is wrong...');
        }
        if (test(val)) {
          window.clearInterval(handler);
          resolve();
        }
      }, this.INTERVAL);
    });
  }

  private async translationValueIs(value: string | null): Promise<void> {
    return await this.translationValuePassesTest(v => {
      console.log('::translationValueIs()', { v, value });
      return v === value;
    });
  }

  private async translationValueIsNot(value: string | null): Promise<void> {
    return await this.translationValuePassesTest(
      v => !isNil(v) && v !== value && !v.endsWith('...')
    );
  }

  private async resetInput() {
    const id = this.uuidv4();
    const contains = (v: string | null) => !!v && v.includes(id);
    await this.setInput(id);
    await this.translationValuePassesTest(contains);
    await this.setInput('');
    await this.translationValuePassesTest(v => !contains(v));
  }

  private async translate(text: string): Promise<string> {
    const chunks = await this.chunks(text);
    const translated = await this.translateChunks(chunks);
    const translation = translated.join(' ');
    const clean = trim(translation);
    return clean;
  }

  private async translateChunks(chunks: string[]): Promise<string[]> {
    const translated = [];
    for (const chunk of chunks) {
      translated.push(await this.translateChunk(chunk));
    }
    return translated;
  }

  private get translationTarget(): HTMLElement | null {
    return document.querySelector('.result.tlid-copy-target');
  }

  private get translationText(): string | null {
    const target = this.translationTarget;
    if (isNil(target)) {
      return null;
    }
    return trim(target.innerText);
  }

  private uuidv4(): string {
    return trim(`${getRandomArbitrary(1, 99999999999999)}`);
  }
}

new TurboTranslator();
