#!bash

content=`cat ./dist/scripts/turbo-translate.js`
title='// ==UserScript==\n
// @name         Tlumok\n
// @namespace    Tlumok\n
// @version      0.1\n
// @description  Fast translation with google translate\n
// @author       Niedzwiedzw\n
// @match        https://translate.google.com/\n
// @grant        none\n
// @require      /Users/niedzwiedz/programming/turbo-translate/tlumok.js\n
// ==/UserScript==\n\n\n
'

touch ./output.js
echo $title > ./output.js
echo $content >> ./output.js
