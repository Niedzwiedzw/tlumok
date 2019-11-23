#!/bin/zsh

npm run build
dist_path=./dist/scripts/turbo-translate.js
cp $dist_path . && git add . && git commit --amend && ggfl