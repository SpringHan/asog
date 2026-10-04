## 简述

该项目是用于将指定文件夹下的 markdown 文件编译为完整的静态博客网站并输出于指定文件夹的命令行程序。

## 技术栈

该项目的技术栈要求如下：  
1. 编程语言： `Rust 2024 Edition`

## 模块

- 命令行参数解析
该项目要求使用 Rust 的 `clap` crate 实现对命令行参数的读取和解析，要求如下：

1. `-f <path>` 或 `--from <path>`
该参数表示所要获取的 markdown 文件的路径。

2. `-o <path>` 或 `--output <path>`
该参数表示最终输出静态网页的所有文件的路径。


- markdown 编译器
该项目需要 markdown 编译器，优先使用 `markdown` crate，用来将所需的 markdown 文件编译为 html 文件。


- 修饰网页的样式表
该项目需要 css 或者其他更现代的样式表对网页进行修饰。
