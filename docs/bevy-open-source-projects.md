# Bevy 开源大型项目参考

调研时间:2026-08-31。社区里常被提到、体量相对较大的开源 Bevy 项目。

## Emergence

- **仓库**:[Leafwing-Studios/Emergence](https://github.com/Leafwing-Studios/Emergence)
- **类型**:后末日微观世界的"有机工厂建造"游戏——玩家操控一个蜂群意识,管理一个需要适应环境变化的多物种殖民地。
- **技术栈**:Rust + Bevy,代码拆成多个 crate(`emergence_game`、`emergence_lib`、`emergence_macros`),设计文档用 mdbook 维护。
- **规模**:289 star,36 fork,主分支 480+ commits——在 Bevy 生态里属于体量较大、架构相对完整的项目。
- **设计亮点**:
  - 玩法强调"引导而非直接指挥"(nudge rather than command)的间接控制机制
  - 程序化生成的 2.5D 世界,带丰富的环境反馈系统
  - 采用"乐观合并"(optimistic merging)的贡献流程
  - 双 MIT/Apache-2.0 授权(Rust 生态惯例),非字体资源 CC-0
- **⚠️ 现状**:**已于 2023-11-22 归档(archived),只读,不再维护**。适合当作学习大型 Bevy ECS 架构的参考代码,但不要指望它能跑起来对应最新版本的 Bevy,也不会有后续更新。

## Sotora

- **仓库**:[sotora-game/sotora](https://github.com/sotora-game/sotora)
- **类型**:官方性质的"完整游戏"展示项目,目标是拿一个真实游戏来field-test和展示 Bevy 引擎的各项能力,目前处于 WIP(开发中)状态。
- **技术栈**:Rust + Bevy,直接跟踪 Bevy 的 git master 分支(锁定到某个已知可用的 commit),而不是用稳定发行版——意味着它长期贴着 Bevy 最前沿的 API 走。
- **规模**:11 star,7 fork——规模明显小于 Emergence,还很早期。
- **维护状态**:通过 Bevy 官方 Discord 保持开发活跃,欢迎贡献,尤其欢迎"更新 Bevy 依赖版本"的 PR。
- **说明**:README 里几乎没有透露具体玩法机制或架构设计细节,更偏"拿来验证引擎能力的载体",而不是一个有明确游戏设计的项目。
- **授权**:代码 MIT,字体 SIL Open Font License,其他素材 CC BY-SA 2.0。

## Jumpy(Fish Folk)

- **仓库**:[fishfolk/jumpy](https://github.com/fishfolk/jumpy)
- **类型**:2-4 人本地/联机对战的战术性 2D 射击游戏,强调走位和策略而不是手速。
- **技术栈**:Rust + Bevy,有专门的 `dev-optimized` 构建 profile 方便快速迭代。
- **规模**:**1.9k star**,1187+ commits——目前找到的几个项目里体量最大、最接近"完整游戏"的一个。
- **维护状态**:活跃维护,CI/CD 齐全,Discord 社区活跃,有规范的贡献指南和 code of conduct。
- **亮点**:
  - 支持玩家自制武器/关卡/音频等的 modding 系统
  - 浏览器内可直接试玩的 Web 版本
  - 跨平台角色自定义、存档持久化
  - 自带关卡编辑工具和赛事/匹配系统
- 这是目前调研到的几个项目里**最值得作为"大型完整 Bevy 项目"参考**的一个——功能完整度、活跃度、代码规模都明显超过 Emergence 和 Sotora。

## Punchy(Fish Folk)

- **仓库**:[fishfolk/punchy](https://github.com/fishfolk/punchy)
- **类型**:2.5D 横版清版格斗游戏,玩法致敬《小蜜蜂 Little Fighter 2》《热血物语 River City Ransom》。
- **技术栈**:Rust + Bevy,支持 WASM 编译跑在浏览器里。
- **规模**:311 star,673+ commits。
- **维护状态**:活跃维护,CI 齐全,发布节奏规律,仓库里专门有一份 `ARCHITECTURE.md` 讲技术架构,适合直接看这份文档理解项目结构。
- **说明**:和 Jumpy 同属 Fish Folk 团队,两者可以对照着看——一个是射击对战,一个是清版格斗,架构风格接近。

## LostInTime

- **仓库**:[RaminKav/LostInTime](https://github.com/RaminKav/LostInTime)
- **类型**:功能丰富的 roguelike 生存游戏。
- **技术栈**:Rust + Bevy。
- **规模**:151 star,**6 天前刚更新过**——目前调研到的项目里更新最新鲜的一个,说明还在持续开发。
- **说明**:个人/小团队项目,规模没有 Jumpy 大,但生存类游戏系统通常涉及库存、合成、AI、程序化生成等复杂子系统,适合看它是怎么用 ECS 拆解这些系统的。

## gdclone

- **仓库**:[opstic/gdclone](https://github.com/opstic/gdclone)
- **类型**:用 Bevy 重新实现的《Geometry Dash》第三方客户端(不是原创游戏,是复刻一个已有的、机制很吃精确物理判定的知名游戏)。
- **技术栈**:Rust + Bevy。
- **规模**:100+ star。
- **说明**:这类"复刻类"项目的参考价值在于——要在 Bevy 上精确复现另一个引擎(Geometry Dash 原版用 cocos2d-x)的关卡格式解析、物理判定和关卡编辑器兼容性,对 Bevy 的渲染精度、输入时序、性能都是不小的考验,能看到不少"跟其他生态对齐"时踩的坑。

## bevy_fs(飞行模拟器)

- **仓库**:[wesfly/bevy_fs](https://github.com/wesfly/bevy_fs)
- **类型**:开源飞行模拟器。
- **技术栈**:Rust + Bevy + `avian3d`(第三方物理引擎)。
- **亮点**:
  - 支持键盘和手柄输入
  - 用真实世界高程数据(elevation data)生成地形并做碰撞
  - 从 glTF 加载场景,支持自定义属性
  - 3D 座舱视角
  - 带屏幕空间反射的水面渲染
- **说明**:这是目前调研到的项目里**技术复杂度最高**的一个——真实地形、3D 物理、自定义渲染效果都涉及到,适合看"大型 3D 场景 + 第三方物理引擎"怎么和 Bevy ECS 结合。仓库用了 Git LFS 管理大体积美术资源。

## 更多活跃项目一览(共 10 个)

按 star 数排序,补充调研到的另外几个规模较小但看起来有真实活动(commit 数、社区描述、非玩具项目)的项目。**说明清楚工具局限**:GitHub 页面的"Updated X ago"这种相对时间戳,这次用的抓取工具没能稳定拿到,所以下表里"活跃度"主要基于 commit 数量、fork 数、社区描述这些间接信号判断,只有 Jumpy / Punchy / LostInTime 这三个是我确认到了具体更新时间证据的(LostInTime 明确是"6 天前更新过")。

| 项目 | Star | 类型 | 备注 |
|---|---|---|---|
| [fishfolk/jumpy](https://github.com/fishfolk/jumpy) | 1.9k | 2D 战术射击 | 确认活跃,见上文 |
| [fishfolk/punchy](https://github.com/fishfolk/punchy) | 311 | 2.5D 清版格斗 | 确认活跃,见上文 |
| [RaminKav/LostInTime](https://github.com/RaminKav/LostInTime) | 151 | roguelike 生存 | 确认 6 天前更新,见上文 |
| [opstic/gdclone](https://github.com/opstic/gdclone) | 111 | Geometry Dash 复刻 | 299 commits,活跃度未直接确认 |
| [ShenMian/sokoban-rs](https://github.com/ShenMian/sokoban-rs) | 80 | 推箱子 + 自动解算器 | 297 commits,前后端分离架构 |
| [wesfly/kestrel](https://github.com/wesfly/kestrel)(即前文 bevy_fs) | 62 | 飞行模拟器 | 678 commits,真实地形+3D物理 |
| golab | 43 | 卡通风格多人射击游戏 | 仅有 GitHub topics 页给出的简介,未展开细查 |
| [Kraken-Space-Program/Kraken-Space-Program](https://github.com/Kraken-Space-Program/Kraken-Space-Program) | 32 | 航天/造火箭沙盒(KSP 类) | 5 fork,社区驱动,自带 Lua modding API,用 wgpu 渲染 |
| [SOF3/traffloat](https://github.com/SOF3/traffloat) | 26 | 太空殖民地物流模拟 | 65 commits,1 个 open issue、0 PR,活跃度偏低 |
| magiaforge | 19 | 双摇杆大逃杀射击游戏 | 仅有 GitHub topics 页给出的简介,未展开细查 |

## 小结

综合几轮调研,可以按参考目的分类:

- **想看"完整游戏该怎么组织"** → **Fish Folk 的 Jumpy**(1.9k star,活跃维护,modding/联机/关卡编辑器一应俱全),是目前找到的最佳范本,远超 Emergence(已归档)和 Sotora(仅 11 star、跟 master 分支)。
- **想看某个细分系统怎么拆** → Punchy(架构文档现成)、LostInTime(生存游戏的库存/合成系统,更新最活跃)。
- **想看 Bevy 撑不撑得住"硬核"场景** → gdclone(精确物理判定 + 复刻另一引擎的关卡格式)、bevy_fs(真实地形 3D 场景 + 第三方物理引擎 + 自定义渲染效果)。

整体上再次印证 Bevy 生态(pre-1.0,~3 个月一次破坏性大版本)下,真正稳定、长期维护、体量够大的开源项目仍然稀缺——大多数能打的项目都来自个人/小团队,star 数普遍在几十到两千之间,Fish Folk 的两个项目是目前公认的天花板。
