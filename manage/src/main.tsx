import { render } from "./libs/ElementBuilder.ts";
import {
  FileInfo,
  MasterInfo,
  PreSubmitInfo,
  RejectReason,
  rejectReasonToString,
  Status,
  statusToString,
  SubmitInfo,
  SubmitStatus,
  submitStatusToString,
  UserData as UserInfo,
} from "./project.ts";
import { assertNotNull } from "./utils/assertion.ts";

const app = assertNotNull(document.querySelector("#app"), "expect #app");
render(<App />, app);

function App() {
  return (
    <div id="app">
      <ProjectUserList />
      <div class="detail">
        <DetailSubmit />
      </div>
    </div>
  );
}

function MasterDetail() {
  const info: MasterInfo | null = Math.random() > 0.5
    ? {
      user_id: BigInt(0),
      project_id: BigInt(0),
      time: Date.now() / 1000.0,
      comment: "阿巴阿巴",
      file: {
        id: BigInt(0),
        name: "阿巴阿巴.wav",
        url: "/test.flac",
      },
    }
    : null;

  return (
    <div class="detailMaster">
      {info != null
        ? (
          <>
            <div>备注: {info.comment}</div>
            <a href={info.file.url}>{info.file.name}</a>
          </>
        )
        : (
          <>
            <div>
              备注(可选): <input />
            </div>
            <div>
              <button>上传</button>
              <span>upload.wav</span>
            </div>
            <button>提交</button>
          </>
        )}
    </div>
  );
}

function DetailSubmit() {
  const infos: SubmitInfo[] = [];
  const count = Math.max(Math.round(Math.random() * 10), 1);
  for (let index = 0; index < count; index++) {
    const status = index === count - 1
      ? Math.round(Math.random() * SubmitStatus.Rejected)
      : SubmitStatus.Rejected;
    const files: FileInfo[] = [{
      id: BigInt(0),
      name: "阿巴阿巴.ogg",
      url: "/test.flac",
      checked: Math.random() > 0.5,
    }];

    infos.push({
      id: BigInt(index),
      user_id: BigInt(0),
      project_id: BigInt(0),
      time: Date.now() / 1000.0,
      files,
      comment: "阿巴阿巴阿巴阿巴",
      status,
      ...(
        status === SubmitStatus.Rejected
          ? {
            reason: Math.round(Math.random() * RejectReason.RequirementNotMet),
            reasonDetail: Math.random() > 0.5 ? "阿巴阿巴" : undefined,
          }
          : {}
      ),
    });
  }
  infos.sort((a, b) => Number(b.id - a.id));

  return (
    <div class="detailSubmit">
      {infos.map((it) => {
        // deno-lint-ignore jsx-key
        return <DetailSubmitItem {...it} />;
      })}
    </div>
  );
}

function DetailSubmitItem(
  { time, files, status, comment, reason, reasonDetail }: SubmitInfo,
) {
  return (
    <div>
      <div>
        提交时间: {(new Date(time * 1000)).toLocaleString()}
      </div>
      <div>
        {files.map((file) => (
          <div>
            <div>
              <input type="checkbox" checked={file.checked} /> {file.name}
            </div>
            <div>
              <audio controls src={file.url} />
              <button type="button">下载</button>
            </div>
          </div>
        ))}
        <div>
        </div>
      </div>
      <div>
        备注: {comment}
      </div>
      ---
      <div>
        {status === SubmitStatus.None
          ? (
            <>
              <div>
                <input type="radio" defaultChecked /> 拒绝
                <input type="radio" /> 通过
              </div>
              <div>
                原因:
                <input type="radio" defaultChecked /> 设备或环境
                <input type="radio" /> 未达标
                <input type="radio" /> 其他
              </div>
              <div>
                (可选):
                <input type="text" />
              </div>
              <button type="button">
                提交
              </button>
            </>
          )
          : (
            <div>
              {submitStatusToString(status)}
              {status === SubmitStatus.Rejected
                ? (
                  <>
                    {" - "}
                    {rejectReasonToString(reason!)}
                    {reasonDetail != null ? ` - ${reasonDetail}` : ""}
                  </>
                )
                : ""}
            </div>
          )}
      </div>
    </div>
  );
}

function DetailPreSubmit() {
  const infos: PreSubmitInfo[] = [];
  const count = Math.max(Math.round(Math.random() * 10), 1);
  const harmonyGroupIntention = Math.random() > 0.5;
  for (let index = 0; index < count; index++) {
    const status = index === count - 1
      ? Math.round(Math.random() * SubmitStatus.Rejected)
      : SubmitStatus.Rejected;
    infos.push({
      id: BigInt(index),
      user_id: BigInt(0),
      project_id: BigInt(0),
      time: Date.now() / 1000.0,
      file: {
        id: BigInt(0),
        name: "阿巴阿巴.ogg",
        url: "/test.flac",
      },
      harmonyGroupIntention,
      status,
      ...(
        status === SubmitStatus.Passed
          ? {
            group: harmonyGroupIntention
              ? {
                lead: Math.random() > 0.5,
                choir: Math.random() > 0.05,
                harmony: Math.random() > 0.5,
              }
              : {
                lead: Math.random() > 0.5,
                choir: false,
                harmony: Math.random() > 0.5,
              },
          }
          : status === SubmitStatus.Rejected
          ? {
            reason: Math.round(Math.random() * RejectReason.RequirementNotMet),
          }
          : {}
      ),
    });
  }
  infos.sort((a, b) => Number(b.id - a.id));

  return (
    <div class="detailPreSubmit">
      {infos.map((it) => {
        // deno-lint-ignore jsx-key
        return <DetailPreSubmitItem {...it} />;
      })}
    </div>
  );
}

function DetailPreSubmitItem(
  { time, file, harmonyGroupIntention, status, reason }: PreSubmitInfo,
) {
  return (
    <div>
      <div>
        提交时间: {(new Date(time * 1000)).toLocaleString()}
      </div>
      <div>
        <div>
          文件名: {file.name}
        </div>
        <div>
          <audio controls src={file.url} />
        </div>
      </div>
      <div>
        和声组意向: {harmonyGroupIntention ? "是" : "否"}
      </div>
      ---
      <div>
        {status === SubmitStatus.None
          ? (
            <>
              <div>
                <input type="radio" defaultChecked /> 拒绝
                <input type="radio" /> 通过
              </div>
              <div>
                原因:
                <input type="radio" defaultChecked /> 设备或环境
                <input type="radio" /> 未达标
              </div>
              <button type="button">
                提交
              </button>
            </>
          )
          : (
            <div>
              {submitStatusToString(status)}
              {status === SubmitStatus.Rejected
                ? <>{" - "}{rejectReasonToString(reason!)}</>
                : ""}
            </div>
          )}
      </div>
    </div>
  );
}

function DetailNone() {
  return <div class="detailNone">未选择</div>;
}

function ProjectUserList() {
  const users: UserInfo[] = [];
  for (let index = 0; index < 32; index++) {
    users.push({
      user_id: BigInt(index),
      project_id: BigInt(0),
      name: Math.round(Math.random() * 10000000).toString(),
      status: Math.round(Math.random() * Status.Mastered),
      group: {
        lead: Math.random() > 0.5,
        choir: Math.random() > 0.05,
        harmony: Math.random() > 0.5,
      },
    });
  }

  return (
    <div class="userList">
      <table>
        <thead>
          <tr>
            <th>通行证ID</th>
            <th>项目用户名</th>
            <th>状态</th>
            <th>初审信息</th>
            <th>领唱</th>
            <th>合唱</th>
            <th>和声</th>
            <th>正式音频</th>
            <th>已修对音频</th>
          </tr>
        </thead>
        <tbody>
          {users.map((it) => {
            // deno-lint-ignore jsx-key
            return <ProjectUserData {...it} />;
          })}
        </tbody>
      </table>
    </div>
  );
}

function ProjectUserData(
  { user_id: id, name, status, group }: UserInfo,
) {
  return (
    <tr>
      <td>{id.toString()}</td>
      <td>{name}</td>
      <td>{statusToString(status)}</td>
      {status >= Status.PreSubmitted
        ? (
          <td>
            <button type="button">详情</button>
          </td>
        )
        : <td></td>}
      {status >= Status.PreSubmitPassed
        ? (
          <>
            <td>
              <input type="checkbox" checked={group.lead}></input>
            </td>
            <td>
              <input type="checkbox" checked={group.choir}></input>
            </td>
            <td>
              <input type="checkbox" checked={group.harmony}></input>
            </td>
          </>
        )
        : (
          <>
            <td></td>
            <td></td>
            <td></td>
          </>
        )}
      {status >= Status.Submitted
        ? (
          <td>
            <button type="button">详情</button>
          </td>
        )
        : <td></td>}
      {status >= Status.SubmitPassed
        ? (
          <td>
            <button type="button">详情</button>
          </td>
        )
        : <td></td>}
    </tr>
  );
}
