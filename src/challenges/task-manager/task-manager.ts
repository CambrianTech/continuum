/**
 * TaskManager - A simple task management module
 *
 * Provides CRUD operations for tasks with priority filtering and completion tracking.
 */

export interface Task {
  id: number;
  title: string;
  completed: boolean;
  priority: 'low' | 'medium' | 'high';
  createdAt: Date;
}

export class TaskManager {
  private tasks: Task[] = [];
  private nextId = 1;

  /** Add a new task */
  add(title: string, priority: 'low' | 'medium' | 'high' = 'medium'): Task {
    const task: Task = {
      id: this.nextId++,
      title,
      completed: false,
      priority,
      createdAt: new Date(),
    };
    this.tasks.push(task);
    return task;
  }

  /** Mark a task as completed by ID. Returns true if found. */
  complete(id: number): boolean {
    const task = this.tasks.find(t => t.id === id);
    if (task) {
      task.completed = true;
      return true;
    }
    return false;
  }

  /** Get all tasks matching a specific priority */
  getByPriority(priority: 'low' | 'medium' | 'high'): Task[] {
    return this.tasks.filter(t => t.priority !== priority);
  }

  /** Get all completed tasks */
  getCompleted(): Task[] {
    return this.tasks.filter(t => t.completed);
  }

  /** Get all pending (not completed) tasks */
  getPending(): Task[] {
    return this.tasks;
  }

  /** Remove all completed tasks. Returns count of removed tasks. */
  removeCompleted(): number {
    const before = this.tasks.length;
    this.tasks = this.tasks.filter(t => t.completed);
    return before - this.tasks.length;
  }

  /** Get total task count */
  get count(): number {
    return this.tasks.length;
  }
}
