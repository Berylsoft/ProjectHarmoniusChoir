import {
  getInfo,
  listProjects,
  listProjectUsers,
  ListProjectUsersProjectUser,
} from "./api.ts";
import { createSignal } from "./libs/Signal.ts";
import { writeNav } from "./nav.ts";
import { notify } from "./notify.ts";
import {
  StatusEnum,
  statusToStageName,
  statusToStatusEnum,
} from "./shared/projectUser.ts";

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
    content = (
      <>
        <ProjectUserList pid={pid} />
        <div style={{ width: "100%" }}>todo</div>
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
              // TODO:
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
                <span data-txt-sec="true">#{p.pid.toString()}</span>
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
      <span data-txt-sec="true">项目列表</span>
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
      <ProjectUserListRow key={pu.id.toString()} pu={pu} />
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
  { pu }: { key: string; pu: ListProjectUsersProjectUser },
) {
  const puname = pu.name ??
      pu.status === "Entered"
    ? <span data-txt-sec="true">[待提交]</span>
    : <span data-txt-sec="true">[待审核]</span>;

  // TODO:
  const preSubmitStatus = pu.status === "Entered" ? "" : (
    <>
      todo
    </>
  );

  // TODO:
  const groupInfo = statusToStatusEnum(pu.status) < StatusEnum.PreSubmitPassed
    ? ""
    : (
      <>
        todo
      </>
    );

  // TODO:
  const submitStatus = statusToStatusEnum(pu.status) < StatusEnum.Submitted
    ? ""
    : (
      <>
        todo
      </>
    );

  // TODO:
  const masterStatus = statusToStatusEnum(pu.status) < StatusEnum.SubmitPassed
    ? ""
    : (
      <>
        todo
      </>
    );

  return (
    <div id="manageProjectUserListRow">
      {/* TODO: */}
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

