import {
  getInfo,
  listProjects,
  listProjectUsers,
  ListProjectUsersProjectUser,
  openFile,
  PreSubmitInfo,
  preSubmitInfo,
  preSubmitReview,
  SubmitInfo,
  submitInfo,
  submitReview,
} from "./api.ts";
import { createSignal } from "./libs/Signal.ts";
import { readNav, writeNav } from "./nav.ts";
import { notify } from "./notify.ts";
import {
  StatusEnum,
  statusToStageName,
  statusToStatusEnum,
} from "./shared/projectUser.ts";
import {
  GroupInfo,
  groupInfoTxt,
  PreSubmitRejectReason,
  preSubmitRejectReasonTxt,
  PreSubmitStatus,
  SubmitRejectReason,
  submitRejectReasonTxt,
  SubmitStatus,
} from "./shared/submit.ts";
import { assert, assertNotNull, unreachable } from "./utils/assertion.ts";

export function Manage(props: { nav: URL }) {
  const nav = props.nav;
  const pidP = Number.parseInt(nav.searchParams.get("pid") ?? "NaN");
  const pid = Number.isSafeInteger(pidP) && pidP >= 0 ? pidP : null;

  const mid = createSignal("loading");
  getInfo().then((it) => mid.set(it.id.toString()));

  let content;
  if (pid === null) {
    content = <ProjectList nav={nav} />;
  } else {
    const detail = readDetail(pid);
    content = (
      <>
        <ProjectUserList pid={pid} />
        <Detail detail={detail} />
      </>
    );
  }

  return (
    <div id="manage">
      <div id="manageNavBar">
        <div id="manageNavBarInfo">
          MID: <span sub:innerText={mid} />
        </div>
        <div id="manageNavBarBtns">
          {pid !== null
            ? <button type="button" class="manageBtn">打包任务</button>
            : ""}
          <button
            type="button"
            class="manageBtn"
            on:click={() => {
              nav.searchParams.delete("pid");
              writeNav(nav, true);
            }}
          >
            项目列表
          </button>
          <button
            type="button"
            class="manageBtn"
            on:click={() => {
              // TODO: logout
              notify("TODO");
            }}
          >
            登出
          </button>
        </div>
      </div>
      <div id="manageContent">
        {content}
      </div>
    </div>
  );
}

function ProjectList(props: { nav: URL }) {
  const items = createSignal(
    <>
      <div>loading</div>
    </>,
  );

  listProjects().then((res) => {
    if (res.projects.length > 0) {
      items.set(
        res.projects.map((p, idx) => (
          <div key={p.pid.toString()}>
            <div class="manageProjectListItemContent">
              <div>
                {p.name}
                <span class="txtSec">#{p.pid.toString()}</span>
              </div>
              <button
                type="button"
                class="manageBtn"
                on:click={() => {
                  props.nav.searchParams.set("pid", p.pid.toString());
                  writeNav(props.nav, true);
                }}
              >
                进入管理
              </button>
            </div>
            {idx < res.projects.length - 1
              ? <div class="manageProjectListItemSep" />
              : ""}
          </div>
        )),
      );
    } else {
      items.set("什么都木有");
    }
  });

  return (
    <div id="manageProjectListPanel">
      <span class="txtSec">项目列表</span>
      <div id="manageProjectList" sub:jsxContent={items}>
      </div>
    </div>
  );
}

function ProjectUserList(props: { pid: number }) {
  const hdr = (
    <div key="hdr" id="manageProjectUserListHdr">
      <div>打包</div>
      <div>UID</div>
      <div>项目UID</div>
      <div>项目用户名</div>
      <div>阶段</div>
      <div>初审提交</div>
      <div>分组详情</div>
      <div>正式提交</div>
      <div>修对</div>
    </div>
  );

  const content = createSignal(
    <>
      {hdr}
    </>,
  );

  listProjectUsers({
    pid: props.pid,
    // TODO: sort & filter
  }).then((res) => {
    const list = res.project_users.map((pu) => (
      <ProjectUserListRow
        key={pu.id.toString()}
        pu={pu}
      />
    ));

    content.set(
      [
        hdr,
        ...list,
      ],
    );
  });

  return (
    <div id="manageProjectUserPanel">
      <div id="manageProjectUserOps">ops</div>
      <div id="manageProjectUserList" sub:jsxContent={content} />
    </div>
  );
}

function ProjectUserListRow(
  { pu }: {
    key: string;
    pu: ListProjectUsersProjectUser;
  },
) {
  const nav = readNav();
  const statusEnum = statusToStatusEnum(pu.status);

  const puname = pu.name ??
    (pu.status === "Entered" || pu.status === "PreSubmitRejected"
      ? <span class="txtSec">[待提交]</span>
      : <span class="txtSec">[待审核]</span>);

  const submitDetailBtn = (
    ty: "PreSubmit" | "Submit" | "Master",
    isCreate: boolean = false,
  ) => (
    <button
      type="button"
      class="manageInfoBtn"
      on:click={() => {
        nav.searchParams.set("detailTy", ty);
        nav.searchParams.set("puid", pu.id.toString());
        writeNav(nav, true);
      }}
    >
      {isCreate ? "创建" : "详情"}
    </button>
  );

  const reject = () => <span class="txtErr">已拒绝</span>;
  const passed = () => <span class="txtOk">已通过</span>;

  let preSubmitStatus = statusEnum === StatusEnum.Entered
    ? ""
    : statusEnum === StatusEnum.PreSubmitted
    ? "已提交"
    : statusEnum === StatusEnum.PreSubmitRejected
    ? reject()
    : passed();
  if (preSubmitStatus !== "") {
    preSubmitStatus = (
      <>
        {preSubmitStatus}
        {submitDetailBtn("PreSubmit")}
      </>
    );
  }

  const groupInfo = statusEnum < StatusEnum.PreSubmitPassed ? "" : (() => {
    assert(
      pu.group_info !== null,
      "expect group_info when pre-submit passed",
    );
    return (
      <>
        {Object.entries(pu.group_info).map((
          [k, v],
        ) => (
          <span key={k} class={v ? "" : "txtSec"}>
            {groupInfoTxt[k as keyof GroupInfo]}
          </span>
        ))}
      </>
    );
  })();

  let submitStatus = statusEnum < StatusEnum.Submitted
    ? ""
    : statusEnum === StatusEnum.Submitted
    ? "已提交"
    : statusEnum === StatusEnum.SubmitRejected
    ? reject()
    : passed();
  if (submitStatus !== "") {
    submitStatus = <>{submitStatus}{submitDetailBtn("Submit")}</>;
  }

  // TODO: master status
  const masterStatus = statusEnum < StatusEnum.SubmitPassed
    ? ""
    : statusEnum === StatusEnum.SubmitPassed
    ? submitDetailBtn("Master", true)
    : submitDetailBtn("Master");

  return (
    <div id="manageProjectUserListRow">
      {/* TODO: bundle checkbox */}
      <div>todo</div>
      <div>{pu.uid.toString()}</div>
      <div>{pu.id.toString()}</div>
      <div>{puname}</div>
      <div>{statusToStageName(pu.status)}</div>
      <div>{preSubmitStatus}</div>
      <div>{groupInfo}</div>
      <div>{submitStatus}</div>
      <div>{masterStatus}</div>
    </div>
  );
}

type DetailParam =
  | { ty: "PreSubmit" | "Submit" | "Master"; puid: number; pid: number }
  | { ty: "Bundle"; pid: number }
  | { ty: "" };

function readDetail(pid: number): DetailParam {
  const nav = readNav();
  const detailTy = nav.searchParams.get("detailTy") ?? "";
  let detail: DetailParam = { ty: "" };
  switch (detailTy) {
    case "PreSubmit":
    case "Submit":
    case "Master": {
      const puidP = Number.parseInt(nav.searchParams.get("puid") ?? "NaN");
      const puid = Number.isSafeInteger(puidP) && puidP >= 0 ? puidP : null;
      if (puid === null) {
        break;
      }

      detail = {
        ty: detailTy,
        puid,
        pid,
      };
      break;
    }
    case "Bundle": {
      detail = {
        ty: detailTy,
        pid,
      };
      break;
    }
  }

  return detail;
}

function Detail(props: { detail: DetailParam }) {
  const detail = props.detail;

  let titleTxt = "None";
  switch (detail.ty) {
    case "PreSubmit": {
      titleTxt = "初审提交";
      break;
    }
    case "Submit": {
      titleTxt = "正式提交";
      break;
    }
    case "Master": {
      titleTxt = "修对";
      break;
    }
    case "Bundle": {
      titleTxt = "打包任务";
      break;
    }
    case "": {
      return <div id="manageDetailNone">-</div>;
    }
  }

  const manageDetail = createSignal(<>""</>);

  const reload = createSignal<() => void>(() => {
    unreachable("uninit");
  });

  const nav = readNav();
  const title = (
    <div id="manageDetailTitle">
      <span class="txtSec">{titleTxt}</span>
      <div id="manageDetailTitleBtns">
        <button
          type="button"
          class="manageBtn"
          on:click={() => {
            reload.get()();
          }}
        >
          刷新
        </button>
        <button
          type="button"
          class="manageBtn"
          on:click={() => {
            nav.searchParams.delete("detailTy");
            writeNav(nav, true);
          }}
        >
          关闭
        </button>
      </div>
    </div>
  );

  function render() {
    let content;
    switch (detail.ty) {
      case "PreSubmit": {
        content = <DetailPreSubmit puid={detail.puid} pid={detail.pid} />;
        break;
      }
      case "Submit": {
        content = <DetailSubmit puid={detail.puid} pid={detail.pid} />;
        break;
      }
      case "Master": {
        content = "todo";
        break;
      }
      case "Bundle": {
        content = "todo";
        break;
      }
    }

    manageDetail.set(<>{title}{content}</>);
  }

  reload.set(() => {
    render();
  });
  render();

  return <div id="manageDetail" sub:jsxContent={manageDetail} />;
}

function DetailPreSubmit({ puid, pid }: { puid: number; pid: number }) {
  const list = createSignal(<>loading</>);

  function renderInfo(submit: PreSubmitInfo) {
    const created_at = (
      <div class="manageSubmitInfoItem">
        提交时间:
        <span>
          {(new Date(submit.created_at)).toLocaleString(undefined, {
            hour12: false,
          })}
        </span>
      </div>
    );

    const uname = (
      <div class="manageSubmitInfoItem">
        待审项目用户名:<span class="manageUserInput">{submit.name}</span>
      </div>
    );

    const harmony_group_intention = submit.harmony_group_intention !== null
      ? (
        <div class="manageSubmitInfoItem">
          和声组意向:
          <span class="manageUserInput">
            {submit.harmony_group_intention ? "是" : "否"}
          </span>
        </div>
      )
      : "";

    const file_info = submit.file_info;
    const file = (
      <div class="manageSubmitInfoItem">
        文件: {file_info !== null
          ? (
            <span class="manageUserInput">
              <button
                type="button"
                class="manageInfoBtn"
                on:click={() => {
                  openFile(pid, file_info.id, "Preview");
                }}
              >
                {file_info.name}
              </button>
            </span>
          )
          : ""}
      </div>
    );

    const comment = (
      <NamedTextArea title="备注" value={submit.comment} readonly />
    );

    const passed: PreSubmitStatus = {
      "Passed": { lead: true, choir: false, harmony: false },
    };
    const reject: PreSubmitStatus = {
      "Rejected": { reason: "DeviceOrEnvironment" },
    };
    const status = createSignal<PreSubmitStatus>(
      submit.status?.status ?? passed,
    );
    const readonly = submit.status !== null;

    function renderSubOps(s: PreSubmitStatus) {
      if ("Passed" in s) {
        const options = Object.keys(groupInfoTxt) as (keyof GroupInfo)[];

        return (
          <>
            <Selector
              multi
              readonly={readonly}
              items={options.map((it) => ({
                name: groupInfoTxt[it],
              }))}
              defaultSelect={options.map((
                it,
                idx,
              ) => (s.Passed[it] ? idx : null)).filter((it) => it !== null)}
              onSelect={(v) => {
                options.forEach((it, idx) => {
                  s.Passed[it] = v.includes(idx);
                });
                status.notify();
              }}
            />
          </>
        );
      } else if ("Rejected" in s) {
        const options = Object.keys(
          preSubmitRejectReasonTxt,
        ) as PreSubmitRejectReason[];

        return (
          <>
            <Selector
              items={options.map((it) => preSubmitRejectReasonTxt[it])}
              defaultSelect={[
                options.findIndex((it) => it === s.Rejected.reason),
              ]}
              readonly={readonly}
              onSelect={(v) => {
                assert(v.length === 1, "expect single select");
                s.Rejected.reason = options[v[0]];
                status.notify();
              }}
            />
          </>
        );
      } else {
        unreachable();
      }
    }
    const subOps = createSignal(renderSubOps(status.get()));
    status.subscribe((v) => subOps.set(renderSubOps(v)));

    const ops = (
      <div class="manageSubmitOps">
        <Selector
          items={[
            { name: "通过", color: "var(--txtOk)" },
            { name: "拒绝", color: "var(--txtErr)" },
          ]}
          readonly={readonly}
          defaultSelect={["Passed" in status.get() ? 0 : 1]}
          onSelect={(v) => {
            switch (v[0]) {
              case 0: {
                status.set(passed);
                break;
              }
              case 1: {
                status.set(reject);
                break;
              }
              default:
                unreachable();
            }
          }}
        />
        <div class="dyn" sub:jsxContent={subOps} />
      </div>
    );

    const submitBtn = readonly
      ? (() => {
        assert(submit.status !== null, "expect status when readonly");
        return (
          <div>
            审核人: <span class="manageUserInput">{submit.status.mname}</span>
          </div>
        );
      })()
      : (
        <button
          type="button"
          class="manageSubmitReviewSubmitBtn"
          on:click={() => {
            preSubmitReview({ pid, sid: submit.id, status: status.get() }).then(
              () => {
                globalThis.location.reload();
              },
            );
          }}
        >
          提交
        </button>
      );

    return (
      <div key={submit.id.toString()}>
        <div class="manageSubmitInfo">
          {created_at}
          {uname}
          {harmony_group_intention}
          {file}
          {comment}
        </div>
        <div class="manageSubmitSep" />
        <div class="manageSubmitReview">
          {ops}
          {submitBtn}
        </div>
      </div>
    );
  }

  preSubmitInfo({ puid }).then((it) => {
    list.set(
      it.pre_submits.map(renderInfo),
    );
  });

  return <div id="manageDetailContent" sub:jsxContent={list} />;
}

function NamedTextArea(
  { title, readonly, value, placeholder, onChange }: {
    title: string;
    readonly?: boolean;
    value?: string;
    placeholder?: string;
    onChange?: (value: string) => void;
  },
) {
  return (
    <div classes={["manageNamedTextArea"]}>
      <span>{title}</span>
      {!readonly
        ? (
          <textarea
            defaultValue={value ?? ""}
            placeholder={placeholder ?? ""}
            on:change={(e) => {
              if (onChange) {
                const target = assertNotNull(e.target);
                assert(
                  target instanceof HTMLTextAreaElement,
                  "expect textarea",
                );
                onChange(target.value);
              }
            }}
          />
        )
        : <div>{value}</div>}
    </div>
  );
}

type SelectorItem = {
  name: string;
  color?: CSSStyleDeclaration["color"];
} | string;
function Selector(
  { items, multi, defaultSelect, readonly, onSelect }: {
    items: SelectorItem[];
    multi?: boolean;
    defaultSelect?: number[];
    readonly?: boolean;
    onSelect?: (idx: number[]) => void;
  },
) {
  const current = createSignal(
    defaultSelect && defaultSelect.length > 0 ? defaultSelect : [0],
  );
  if (onSelect) {
    current.subscribe(onSelect);
  }

  const getColor = (idx: number, it: SelectorItem) => {
    let color;
    if (typeof it === "string") {
      color = "var(--txt)";
    } else {
      color = it.color ?? "var(--txt)";
    }
    return current.get().includes(idx) ? color : "var(--txtSec)";
  };

  return (
    <div class="manageSelector">
      {items.map((it, idx) => {
        return (
          <button
            key={idx.toString()}
            type="button"
            on:click={() => {
              if (readonly) return;

              if (multi) {
                current.update((prev) => {
                  if (prev.includes(idx)) {
                    if (prev.length > 1) {
                      return prev.filter((it) => it != idx);
                    } else {
                      return prev;
                    }
                  } else {
                    return [...prev, idx];
                  }
                });
              } else {
                current.update((prev) => (prev[0] === idx ? prev : [idx]));
              }
            }}
            with={(ref) => {
              ref.style("color", getColor(idx, it));
              current.subscribe(() => {
                ref.style("color", getColor(idx, it));
              });
            }}
          >
            {typeof it === "string" ? it : it.name}
          </button>
        );
      })}
    </div>
  );
}

function DetailSubmit({ puid, pid }: { puid: number; pid: number }) {
  type CheckedFiles = {
    isChecked(id: number): boolean;
    set(id: number, checked: boolean): void;
    getAll(): number[];
  };

  const list = createSignal(<>loading</>);

  function renderInfo(
    submit: SubmitInfo,
    checked_files: CheckedFiles,
  ) {
    const created_at = (
      <div class="manageSubmitInfoItem">
        提交时间:
        <span>
          {(new Date(submit.created_at)).toLocaleString(undefined, {
            hour12: false,
          })}
        </span>
      </div>
    );

    const comment = (
      <NamedTextArea title="备注" value={submit.comment} readonly />
    );

    const files = (
      <div class="manageSubmitFileList">
        {submit.files.map((file) => {
          let checked = checked_files.isChecked(file.id);
          return (
            <div class="manageSubmitFile">
              <div
                data-checked={checked.toString()}
                with={(ref) => {
                  ref.on("click", () => {
                    checked = !checked;
                    checked_files.set(file.id, checked);
                    if (checked) {
                      ref.data("checked", "true");
                    } else {
                      ref.data("checked", "false");
                    }
                  });
                }}
              />
              <span
                class="txtInfo"
                on:click={() => {
                  openFile(pid, file.id, "Preview");
                }}
              >
                {file.name}
              </span>
            </div>
          );
        })}
      </div>
    );

    const passed: SubmitStatus = "Passed";
    const reject: SubmitStatus = {
      "Rejected": { reason: "Other", detail: null },
    };
    const status = createSignal<SubmitStatus>(
      submit.status?.status ?? passed,
    );
    const readonly = submit.status !== null;

    function renderSubOps(s: SubmitStatus) {
      if ("Passed" === s) {
        return "";
      } else if ("Rejected" in s) {
        const options = Object.keys(
          submitRejectReasonTxt,
        ) as SubmitRejectReason[];

        return (
          <>
            <Selector
              items={options.map((it) => submitRejectReasonTxt[it])}
              defaultSelect={[
                options.findIndex((it) => it === s.Rejected.reason),
              ]}
              readonly={readonly}
              onSelect={(v) => {
                assert(v.length === 1, "expect single select");
                s.Rejected.reason = options[v[0]];
                status.notify();
              }}
            />
            <NamedTextArea
              title="详细说明(可选)"
              value={s.Rejected.detail ?? ""}
              placeholder="一些说明, 或者不写"
              readonly={readonly}
              onChange={(txt) => {
                const trimmed = txt.trim();
                if (trimmed === "") {
                  s.Rejected.detail = null;
                } else {
                  s.Rejected.detail = trimmed;
                }
              }}
            />
          </>
        );
      } else {
        unreachable();
      }
    }
    const subOps = createSignal(renderSubOps(status.get()));
    status.subscribe((v) => subOps.set(renderSubOps(v)));

    const ops = (
      <div class="manageSubmitOps">
        <Selector
          items={[
            { name: "通过", color: "var(--txtOk)" },
            { name: "拒绝", color: "var(--txtErr)" },
          ]}
          readonly={readonly}
          defaultSelect={["Passed" === status.get() ? 0 : 1]}
          onSelect={(v) => {
            switch (v[0]) {
              case 0: {
                status.set(passed);
                break;
              }
              case 1: {
                status.set(reject);
                break;
              }
              default:
                unreachable();
            }
          }}
        />
        <div class="dyn" sub:jsxContent={subOps} />
      </div>
    );

    const submitBtn = readonly
      ? (() => {
        assert(submit.status !== null, "expect status when readonly");
        return (
          <div>
            审核人: <span class="manageUserInput">{submit.status.mname}</span>
          </div>
        );
      })()
      : (
        <button
          type="button"
          class="manageSubmitReviewSubmitBtn"
          on:click={() => {
            submitReview({
              pid,
              sid: submit.id,
              status: status.get(),
              checked_files: checked_files.getAll(),
            }).then(
              () => {
                globalThis.location.reload();
              },
            );
          }}
        >
          提交
        </button>
      );

    return (
      <div key={submit.id.toString()}>
        <div class="manageSubmitInfo">
          {created_at}
          {comment}
          {files}
        </div>
        <div class="manageSubmitSep" />
        <div class="manageSubmitReview">
          {ops}
          {submitBtn}
        </div>
      </div>
    );
  }

  submitInfo({ puid }).then((res) => {
    const checked_files_set = new Set(res.checked_files);
    const checked_files: CheckedFiles = {
      isChecked(id): boolean {
        return checked_files_set.has(id);
      },
      set(id, checked) {
        if (checked) {
          checked_files_set.add(id);
        } else {
          checked_files_set.delete(id);
        }
      },
      getAll(): number[] {
        return Array.from(checked_files_set);
      },
    };

    list.set(
      res.submits
        .map((submit) => renderInfo(submit, checked_files)),
    );
  });

  return <div id="manageDetailContent" sub:jsxContent={list} />;
}
