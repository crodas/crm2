import { useState, useRef } from 'react'

interface DragListProps {
  children: React.ReactNode[]
  keys: (number | string)[]
  onReorder: (ids: (number | string)[]) => void
}

export default function DragList({ children, keys, onReorder }: DragListProps) {
  const [dragIdx, setDragIdx] = useState<number | null>(null)
  const [overIdx, setOverIdx] = useState<number | null>(null)
  const dragNode = useRef<HTMLTableRowElement | null>(null)

  const handleDragStart = (idx: number, e: React.DragEvent<HTMLTableRowElement>) => {
    setDragIdx(idx)
    dragNode.current = e.currentTarget
    e.dataTransfer.effectAllowed = 'move'
  }

  const handleDragOver = (idx: number, e: React.DragEvent) => {
    e.preventDefault()
    e.dataTransfer.dropEffect = 'move'
    if (dragIdx !== null && idx !== dragIdx) {
      setOverIdx(idx)
    }
  }

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault()
    if (dragIdx === null || overIdx === null || dragIdx === overIdx) {
      setDragIdx(null)
      setOverIdx(null)
      return
    }
    const reordered = [...keys]
    const [moved] = reordered.splice(dragIdx, 1)
    reordered.splice(overIdx, 0, moved)
    onReorder(reordered)
    setDragIdx(null)
    setOverIdx(null)
  }

  const handleDragEnd = () => {
    setDragIdx(null)
    setOverIdx(null)
  }

  return (
    <tbody>
      {children.map((child, idx) => (
        <tr
          key={keys[idx]}
          draggable
          onDragStart={e => handleDragStart(idx, e)}
          onDragOver={e => handleDragOver(idx, e)}
          onDrop={handleDrop}
          onDragEnd={handleDragEnd}
          style={{
            cursor: 'grab',
            opacity: dragIdx === idx ? 0.4 : 1,
            borderTop: overIdx === idx && dragIdx !== null && dragIdx > idx ? '2px solid var(--primary)' : undefined,
            borderBottom: overIdx === idx && dragIdx !== null && dragIdx < idx ? '2px solid var(--primary)' : undefined,
          }}
        >
          {child}
        </tr>
      ))}
    </tbody>
  )
}
