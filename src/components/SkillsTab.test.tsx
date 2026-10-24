import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { I18nProvider } from "../i18n";
import { makeProject, makeResource } from "../test/fixtures";
import type { Project } from "../types";
import { SkillsTab } from "./SkillsTab";

function renderTab(project: Project, send = vi.fn().mockResolvedValue({}), lang: "vi" | "en" = "vi") {
  render(
    <I18nProvider lang={lang}>
      <SkillsTab project={project} send={send} />
    </I18nProvider>,
  );
  return { send, user: userEvent.setup() };
}

const SKILLS = [
  { id: 1, name: "Thiết kế" },
  { id: 2, name: "Kiểm thử" },
];
const team = (skills = SKILLS) =>
  makeProject({
    skills,
    resources: [makeResource({ uid: 10, name: "An", skills: [1] }), makeResource({ uid: 11, name: "Bình" })],
  });

describe("SkillsTab", () => {
  it("says when there is no skill and adds one with Enter", async () => {
    const { send, user } = renderTab(team([]));
    expect(screen.getByText("Chưa có kỹ năng nào.")).toBeInTheDocument();
    expect(screen.queryByRole("table")).not.toBeInTheDocument();
    const name = screen.getByRole("textbox", { name: "Kỹ năng mới" });
    await user.type(name, "  Vận hành {Enter}");
    expect(send).toHaveBeenCalledWith({ type: "add_skill", name: "Vận hành" });
    expect(name).toHaveValue("");
  });

  it("keeps a name the engine refused, and refuses a comma itself", async () => {
    const { send, user } = renderTab(team([]), vi.fn().mockResolvedValue(undefined));
    const name = screen.getByRole("textbox", { name: "Kỹ năng mới" });
    await user.type(name, "Web{Enter}");
    expect(name).toHaveValue("Web");
    await user.clear(name);
    await user.type(name, "C, C++");
    expect(name).toHaveAttribute("aria-invalid", "true");
    expect(screen.getByRole("button", { name: "Thêm kỹ năng" })).toBeDisabled();
    expect(send).toHaveBeenCalledTimes(1);
  });

  it("renames a skill on leaving its name and keeps the name when emptied", async () => {
    const { send, user } = renderTab(team());
    const name = screen.getByRole("textbox", { name: "Tên kỹ năng Thiết kế" });
    await user.clear(name);
    await user.type(name, "Thiết kế UI");
    await user.tab();
    expect(send).toHaveBeenLastCalledWith({ type: "rename_skill", id: 1, name: "Thiết kế UI" });
    await user.clear(name);
    await user.tab();
    expect(send).toHaveBeenCalledTimes(1);
    expect(name).toHaveValue("Thiết kế");
  });

  it("puts the name back when the engine refuses a rename", async () => {
    const { send, user } = renderTab(team(), vi.fn().mockResolvedValue(undefined));
    const name = screen.getByRole("textbox", { name: "Tên kỹ năng Thiết kế" });
    await user.clear(name);
    await user.type(name, "Kiểm thử");
    await user.tab();
    expect(send).toHaveBeenCalledWith({ type: "rename_skill", id: 1, name: "Kiểm thử" });
    await waitFor(() => expect(name).toHaveValue("Thiết kế"));
  });

  it("deletes a skill", async () => {
    const { send, user } = renderTab(team());
    await user.click(screen.getByRole("button", { name: "Xóa kỹ năng Kiểm thử" }));
    expect(send).toHaveBeenCalledWith({ type: "delete_skill", id: 2 });
  });

  it("ticks who has which skill, building on the list sent last", async () => {
    const { send, user } = renderTab(team());
    expect(screen.getByRole("checkbox", { name: "An có Thiết kế" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Bình có Thiết kế" })).not.toBeChecked();
    // Two quick clicks before the engine answers both count.
    await user.click(screen.getByRole("checkbox", { name: "An có Kiểm thử" }));
    await user.click(screen.getByRole("checkbox", { name: "An có Thiết kế" }));
    expect(send).toHaveBeenNthCalledWith(1, { type: "set_resource_skills", uid: 10, skills: [1, 2] });
    expect(send).toHaveBeenNthCalledWith(2, { type: "set_resource_skills", uid: 10, skills: [2] });
  });

  it("edits the scale of levels, keeping each level by its id", async () => {
    const levels = [
      { id: 1, name: "Junior" },
      { id: 2, name: "Senior" },
    ];
    const { send, user } = renderTab(makeProject({ ...team(), skill_levels: levels }));
    expect(screen.getByRole("list", { name: "Các cấp độ, từ thấp đến cao" })).toBeInTheDocument();
    await user.type(screen.getByRole("textbox", { name: "Cấp độ mới" }), "Mid{Enter}");
    expect(send).toHaveBeenLastCalledWith({
      type: "set_skill_levels",
      levels: [...levels, { id: null, name: "Mid" }],
    });
    await user.click(screen.getByRole("button", { name: "Đưa Senior lên thấp hơn" }));
    // Each change builds on the scale sent last, Mid included, before the engine answers.
    const mid = { id: null, name: "Mid" };
    expect(send).toHaveBeenLastCalledWith({ type: "set_skill_levels", levels: [levels[1], levels[0], mid] });
    await user.click(screen.getByRole("button", { name: "Xóa cấp độ Junior" }));
    expect(send).toHaveBeenLastCalledWith({ type: "set_skill_levels", levels: [levels[1], mid] });
  });

  it("refuses a level name with a colon before sending it", async () => {
    const { send, user } = renderTab(team());
    await user.type(screen.getByRole("textbox", { name: "Cấp độ mới" }), "A:B");
    expect(screen.getByRole("button", { name: "Thêm cấp độ" })).toBeDisabled();
    expect(send).not.toHaveBeenCalled();
  });

  it("chooses each person's level per skill once the project has a scale", async () => {
    const project = makeProject({
      skills: SKILLS,
      skill_levels: [
        { id: 1, name: "Junior" },
        { id: 2, name: "Senior" },
      ],
      resources: [makeResource({ uid: 10, name: "An", skills: [1, 2], skill_levels: { "1": 2 } })],
    });
    const { send, user } = renderTab(project);
    expect(screen.queryByRole("checkbox")).not.toBeInTheDocument();
    const design = screen.getByRole("combobox", { name: "Cấp độ Thiết kế của An" });
    expect(design).toHaveValue("2");
    expect(screen.getByRole("combobox", { name: "Cấp độ Kiểm thử của An" })).toHaveValue("any");
    await user.selectOptions(screen.getByRole("combobox", { name: "Cấp độ Kiểm thử của An" }), "Junior");
    expect(send).toHaveBeenLastCalledWith({
      type: "set_resource_skills",
      uid: 10,
      skills: [1, 2],
      levels: [
        { skill: 1, level: 2 },
        { skill: 2, level: 1 },
      ],
    });
    await user.selectOptions(design, "—");
    expect(send).toHaveBeenLastCalledWith({
      type: "set_resource_skills",
      uid: 10,
      skills: [2],
      levels: [{ skill: 2, level: 1 }],
    });
  });

  it("speaks English", () => {
    renderTab(team(), undefined, "en");
    expect(screen.getByRole("table", { name: "Who has which skill" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Bình has Kiểm thử" })).toBeInTheDocument();
  });
});
