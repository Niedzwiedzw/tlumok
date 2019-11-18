'use strict';
const path = require('path');

const CopyWebpackPlugin = require('copy-webpack-plugin');
const MiniCssExtractPlugin = require('mini-css-extract-plugin');
const HtmlWebpackPlugin = require('html-webpack-plugin');


const mode = 'production';

module.exports = {
  entry: {
    'turbo-translate': ['scripts/app.ts'],
  },

  context: path.join(process.cwd(), 'src'),

  output: {
    publicPath: mode === 'production' ? '/' : 'http://localhost:8080/',
    path: path.join(process.cwd(), 'dist'),
    filename: 'scripts/[name].js',
  },

  mode,

  module: {
    rules: [
      {
        test: /\.ts$/,
        loader: 'ts-loader',
      },
      {
        test: /\.(css|sass|scss)$/,
        use: [
          MiniCssExtractPlugin.loader,
          {
            loader: 'css-loader',
          },
          {
            loader: 'sass-loader',
          },
        ],
      },
    ],
  },

  plugins: [
    // new HtmlWebpackPlugin({
    //   template: 'public/index.html',
    //   chunksSortMode: 'dependency',
    // }),

    // new MiniCssExtractPlugin({
    //   filename: 'css/[name].[hash].css',
    //   chunkFilename: 'css/[id].[hash].css',
    // }),

    // new CopyWebpackPlugin([{ from: 'public' }]),

  ],

  resolve: {
    modules: ['node_modules', path.resolve(process.cwd(), 'src')],
    extensions: ['.ts', '.js', 'scss'],
  },

  devServer: {
    contentBase: './dist',
    clientLogLevel: 'info',
    port: 8080,
    inline: true,
    historyApiFallback: false,
    watchOptions: {
      aggregateTimeout: 300,
      poll: 500,
    },
  },
	optimization: {
		// We no not want to minimize our code.
		minimize: false
	},
  devtool: 'eval-source-map',
};
